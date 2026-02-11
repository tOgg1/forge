package cli

import (
	"context"
	"database/sql"
	"errors"
	"fmt"
	"path/filepath"
	"strings"
	"time"

	_ "modernc.org/sqlite"
)

type mailStore struct {
	db *sql.DB
}

func openMailStore() (*mailStore, error) {
	cfg := GetConfig()
	if cfg == nil {
		return nil, errors.New("configuration not loaded")
	}

	path := filepath.Join(cfg.Global.ConfigDir, "mail.db")
	dsn := fmt.Sprintf("%s?_pragma=busy_timeout(5000)&_pragma=journal_mode(WAL)&_pragma=foreign_keys(ON)&_pragma=synchronous(NORMAL)", path)

	db, err := sql.Open("sqlite", dsn)
	if err != nil {
		return nil, fmt.Errorf("failed to open mail database: %w", err)
	}
	if err := db.Ping(); err != nil {
		_ = db.Close()
		return nil, fmt.Errorf("failed to connect to mail database: %w", err)
	}

	store := &mailStore{db: db}
	if err := store.ensureSchema(context.Background()); err != nil {
		_ = db.Close()
		return nil, err
	}

	return store, nil
}

func (s *mailStore) Close() error {
	if s == nil || s.db == nil {
		return nil
	}
	return s.db.Close()
}

func (s *mailStore) ensureSchema(ctx context.Context) error {
	statements := []string{
		`CREATE TABLE IF NOT EXISTS mail_messages (
			id INTEGER PRIMARY KEY AUTOINCREMENT,
			project TEXT NOT NULL,
			agent TEXT NOT NULL,
			sender TEXT NOT NULL,
			subject TEXT NOT NULL,
			body TEXT NOT NULL,
			created_at TEXT NOT NULL,
			importance TEXT,
			ack_required INTEGER NOT NULL DEFAULT 0,
			thread_id TEXT
		)`,
		`CREATE TABLE IF NOT EXISTS mail_status (
			project TEXT NOT NULL,
			agent TEXT NOT NULL,
			message_id INTEGER NOT NULL,
			read_at TEXT,
			acked_at TEXT,
			PRIMARY KEY (project, agent, message_id)
		)`,
		`CREATE INDEX IF NOT EXISTS mail_messages_inbox_idx ON mail_messages(project, agent, created_at)`,
		`CREATE INDEX IF NOT EXISTS mail_status_lookup_idx ON mail_status(project, agent, message_id)`,
	}

	for _, stmt := range statements {
		if _, err := s.db.ExecContext(ctx, stmt); err != nil {
			return fmt.Errorf("failed to initialize mail schema: %w", err)
		}
	}
	return nil
}

func (s *mailStore) SendLocal(ctx context.Context, req mailSendRequest) ([]int64, error) {
	if s == nil || s.db == nil {
		return nil, errors.New("mail store unavailable")
	}
	if len(req.To) == 0 {
		return nil, errors.New("no recipients provided")
	}

	now := time.Now().UTC()
	createdAt := now.Format(time.RFC3339Nano)

	stmt, err := s.db.PrepareContext(ctx, `
		INSERT INTO mail_messages (project, agent, sender, subject, body, created_at, importance, ack_required, thread_id)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
	`)
	if err != nil {
		return nil, fmt.Errorf("failed to prepare mail insert: %w", err)
	}
	defer stmt.Close()

	ids := make([]int64, 0, len(req.To))
	for _, recipient := range req.To {
		res, err := stmt.ExecContext(ctx,
			req.Project,
			recipient,
			req.From,
			req.Subject,
			req.Body,
			createdAt,
			normalizeEmpty(req.Priority),
			boolToInt(req.AckRequired),
			"",
		)
		if err != nil {
			return nil, fmt.Errorf("failed to store local message: %w", err)
		}
		id, err := res.LastInsertId()
		if err != nil {
			return nil, fmt.Errorf("failed to read message id: %w", err)
		}
		ids = append(ids, id)
	}

	return ids, nil
}

func (s *mailStore) ListLocal(ctx context.Context, project, agent string, since *time.Time, unreadOnly bool, limit int) ([]mailMessage, error) {
	if s == nil || s.db == nil {
		return nil, errors.New("mail store unavailable")
	}
	if strings.TrimSpace(project) == "" || strings.TrimSpace(agent) == "" {
		return nil, errors.New("project and agent are required")
	}

	query := `
		SELECT m.id, m.sender, m.subject, m.body, m.created_at, m.importance, m.ack_required, m.thread_id,
		       s.read_at, s.acked_at
		FROM mail_messages m
		LEFT JOIN mail_status s
		  ON s.project = m.project AND s.agent = m.agent AND s.message_id = m.id
		WHERE m.project = ? AND m.agent = ?`

	args := []any{project, agent}

	if since != nil && !since.IsZero() {
		query += " AND m.created_at >= ?"
		args = append(args, since.UTC().Format(time.RFC3339Nano))
	}
	if unreadOnly {
		query += " AND s.read_at IS NULL"
	}

	query += " ORDER BY m.created_at DESC"
	if limit > 0 {
		query += " LIMIT ?"
		args = append(args, limit)
	}

	rows, err := s.db.QueryContext(ctx, query, args...)
	if err != nil {
		return nil, fmt.Errorf("failed to query mail inbox: %w", err)
	}
	defer rows.Close()

	var messages []mailMessage
	for rows.Next() {
		var (
			id          int64
			from        string
			subject     string
			body        string
			createdRaw  string
			importance  string
			ackRequired int
			threadID    sql.NullString
			readRaw     sql.NullString
			ackedRaw    sql.NullString
		)

		if err := rows.Scan(&id, &from, &subject, &body, &createdRaw, &importance, &ackRequired, &threadID, &readRaw, &ackedRaw); err != nil {
			return nil, fmt.Errorf("failed to scan mail row: %w", err)
		}

		createdAt := parseMailTime(createdRaw)
		readAt := parseNullableTime(readRaw)
		ackedAt := parseNullableTime(ackedRaw)

		messages = append(messages, mailMessage{
			ID:          id,
			ThreadID:    threadID.String,
			From:        from,
			Subject:     subject,
			Body:        body,
			CreatedAt:   createdAt,
			Importance:  importance,
			AckRequired: ackRequired == 1,
			ReadAt:      readAt,
			AckedAt:     ackedAt,
			Backend:     string(mailBackendLocal),
		})
	}

	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("mail query error: %w", err)
	}

	return messages, nil
}

func (s *mailStore) GetLocal(ctx context.Context, project, agent string, messageID int64) (mailMessage, error) {
	if s == nil || s.db == nil {
		return mailMessage{}, errors.New("mail store unavailable")
	}
	if messageID <= 0 {
		return mailMessage{}, errors.New("message id required")
	}

	query := `
		SELECT m.id, m.sender, m.subject, m.body, m.created_at, m.importance, m.ack_required, m.thread_id,
		       s.read_at, s.acked_at
		FROM mail_messages m
		LEFT JOIN mail_status s
		  ON s.project = m.project AND s.agent = m.agent AND s.message_id = m.id
		WHERE m.project = ? AND m.agent = ? AND m.id = ?`

	var (
		id          int64
		from        string
		subject     string
		body        string
		createdRaw  string
		importance  string
		ackRequired int
		threadID    sql.NullString
		readRaw     sql.NullString
		ackedRaw    sql.NullString
	)

	err := s.db.QueryRowContext(ctx, query, project, agent, messageID).
		Scan(&id, &from, &subject, &body, &createdRaw, &importance, &ackRequired, &threadID, &readRaw, &ackedRaw)
	if err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return mailMessage{}, fmt.Errorf("message %s not found", formatMailID(messageID))
		}
		return mailMessage{}, fmt.Errorf("failed to read mail: %w", err)
	}

	createdAt := parseMailTime(createdRaw)
	readAt := parseNullableTime(readRaw)
	ackedAt := parseNullableTime(ackedRaw)

	return mailMessage{
		ID:          id,
		ThreadID:    threadID.String,
		From:        from,
		Subject:     subject,
		Body:        body,
		CreatedAt:   createdAt,
		Importance:  importance,
		AckRequired: ackRequired == 1,
		ReadAt:      readAt,
		AckedAt:     ackedAt,
		Backend:     string(mailBackendLocal),
	}, nil
}

func (s *mailStore) LoadStatus(ctx context.Context, project, agent string, ids []int64) (map[int64]mailStatus, error) {
	if s == nil || s.db == nil {
		return nil, errors.New("mail store unavailable")
	}
	if len(ids) == 0 {
		return map[int64]mailStatus{}, nil
	}

	placeholders := make([]string, 0, len(ids))
	args := make([]any, 0, len(ids)+2)
	args = append(args, project, agent)
	for _, id := range ids {
		placeholders = append(placeholders, "?")
		args = append(args, id)
	}

	query := fmt.Sprintf(`
		SELECT message_id, read_at, acked_at
		FROM mail_status
		WHERE project = ? AND agent = ? AND message_id IN (%s)
	`, strings.Join(placeholders, ","))

	rows, err := s.db.QueryContext(ctx, query, args...)
	if err != nil {
		return nil, fmt.Errorf("failed to load mail status: %w", err)
	}
	defer rows.Close()

	statuses := make(map[int64]mailStatus, len(ids))
	for rows.Next() {
		var (
			messageID int64
			readRaw   sql.NullString
			ackedRaw  sql.NullString
		)
		if err := rows.Scan(&messageID, &readRaw, &ackedRaw); err != nil {
			return nil, fmt.Errorf("failed to scan mail status: %w", err)
		}
		statuses[messageID] = mailStatus{
			ReadAt:  parseNullableTime(readRaw),
			AckedAt: parseNullableTime(ackedRaw),
		}
	}

	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("mail status query error: %w", err)
	}

	return statuses, nil
}

func (s *mailStore) MarkRead(ctx context.Context, project, agent string, messageID int64, readAt time.Time) error {
	return s.upsertStatus(ctx, project, agent, messageID, readAt, time.Time{})
}

func (s *mailStore) MarkAck(ctx context.Context, project, agent string, messageID int64, ackedAt time.Time) error {
	return s.upsertStatus(ctx, project, agent, messageID, time.Time{}, ackedAt)
}

func (s *mailStore) upsertStatus(ctx context.Context, project, agent string, messageID int64, readAt time.Time, ackedAt time.Time) error {
	if s == nil || s.db == nil {
		return errors.New("mail store unavailable")
	}
	if messageID <= 0 {
		return errors.New("message id required")
	}

	readValue := ""
	ackValue := ""
	if !readAt.IsZero() {
		readValue = readAt.UTC().Format(time.RFC3339Nano)
	}
	if !ackedAt.IsZero() {
		ackValue = ackedAt.UTC().Format(time.RFC3339Nano)
	}

	_, err := s.db.ExecContext(ctx, `
		INSERT INTO mail_status (project, agent, message_id, read_at, acked_at)
		VALUES (?, ?, ?, NULLIF(?, ''), NULLIF(?, ''))
		ON CONFLICT(project, agent, message_id) DO UPDATE SET
			read_at = COALESCE(NULLIF(excluded.read_at, ''), mail_status.read_at),
			acked_at = COALESCE(NULLIF(excluded.acked_at, ''), mail_status.acked_at)
	`, project, agent, messageID, readValue, ackValue)
	if err != nil {
		return fmt.Errorf("failed to update mail status: %w", err)
	}
	return nil
}

func parseNullableTime(value sql.NullString) *time.Time {
	if !value.Valid {
		return nil
	}
	parsed := parseMailTime(value.String)
	if parsed.IsZero() {
		return nil
	}
	return &parsed
}

func boolToInt(value bool) int {
	if value {
		return 1
	}
	return 0
}

func normalizeEmpty(value string) string {
	if strings.TrimSpace(value) == "" {
		return ""
	}
	return value
}
