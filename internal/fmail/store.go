package fmail

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"
)

const (
	maxIDRetries  = 10
	rootDirPerm   = 0o755
	topicDirPerm  = 0o755
	dmDirPerm     = 0o700
	topicFilePerm = 0o644
	dmFilePerm    = 0o600
	agentFilePerm = 0o644
)

type Store struct {
	Root        string
	now         func() time.Time
	idGenerator func(time.Time) string
}

type StoreOption func(*Store)

func WithNow(now func() time.Time) StoreOption {
	return func(store *Store) {
		if now != nil {
			store.now = now
		}
	}
}

func WithIDGenerator(gen func(time.Time) string) StoreOption {
	return func(store *Store) {
		if gen != nil {
			store.idGenerator = gen
		}
	}
}

// NewStore initializes a store rooted at <projectRoot>/.fmail.
func NewStore(projectRoot string, opts ...StoreOption) (*Store, error) {
	if strings.TrimSpace(projectRoot) == "" {
		return nil, fmt.Errorf("project root required")
	}
	abs, err := filepath.Abs(projectRoot)
	if err != nil {
		return nil, err
	}
	store := &Store{
		Root:        filepath.Join(abs, ".fmail"),
		now:         func() time.Time { return time.Now().UTC() },
		idGenerator: GenerateMessageID,
	}
	for _, opt := range opts {
		opt(store)
	}
	return store, nil
}

func (s *Store) EnsureRoot() error {
	return os.MkdirAll(s.Root, rootDirPerm)
}

func (s *Store) TopicDir(topic string) string {
	return filepath.Join(s.Root, "topics", topic)
}

func (s *Store) DMDir(agent string) string {
	return filepath.Join(s.Root, "dm", agent)
}

func (s *Store) AgentsDir() string {
	return filepath.Join(s.Root, "agents")
}

func (s *Store) ProjectFile() string {
	return filepath.Join(s.Root, "project.json")
}

func (s *Store) TopicMessagePath(topic, id string) string {
	return filepath.Join(s.TopicDir(topic), id+".json")
}

func (s *Store) DMMessagePath(agent, id string) string {
	return filepath.Join(s.DMDir(agent), id+".json")
}

func (s *Store) SaveMessage(message *Message) (string, error) {
	if message == nil {
		return "", ErrEmptyMessage
	}

	normalizedFrom, err := NormalizeAgentName(message.From)
	if err != nil {
		return "", err
	}
	message.From = normalizedFrom

	normalizedTarget, isDM, err := NormalizeTarget(message.To)
	if err != nil {
		return "", err
	}
	message.To = normalizedTarget

	if message.Time.IsZero() {
		message.Time = s.now()
	}

	if message.ID == "" {
		message.ID = s.idGenerator(message.Time)
	}

	if err := message.Validate(); err != nil {
		return "", err
	}

	if err := s.EnsureRoot(); err != nil {
		return "", err
	}

	var dir string
	var filePerm os.FileMode
	if isDM {
		agent := strings.TrimPrefix(normalizedTarget, "@")
		var err error
		dir, err = s.ensureDMDir(agent)
		if err != nil {
			return "", err
		}
		filePerm = dmFilePerm
	} else {
		dir = s.TopicDir(normalizedTarget)
		if err := ensureDirPerm(dir, topicDirPerm); err != nil {
			return "", err
		}
		filePerm = topicFilePerm
	}

	for attempt := 0; attempt < maxIDRetries; attempt++ {
		data, err := marshalMessage(message)
		if err != nil {
			return "", err
		}
		if len(data) > MaxMessageSize {
			return "", ErrMessageTooLarge
		}

		path := filepath.Join(dir, message.ID+".json")
		err = writeFileExclusivePerm(path, data, filePerm)
		if err == nil {
			return message.ID, nil
		}
		if errors.Is(err, os.ErrExist) {
			message.ID = s.idGenerator(s.now())
			continue
		}
		return "", err
	}
	return "", ErrIDCollision
}

// SaveMessageExact writes a message using its existing ID, returning true if persisted.
func (s *Store) SaveMessageExact(message *Message) (bool, error) {
	if message == nil {
		return false, ErrEmptyMessage
	}
	if strings.TrimSpace(message.ID) == "" {
		return false, fmt.Errorf("missing id")
	}

	normalizedFrom, err := NormalizeAgentName(message.From)
	if err != nil {
		return false, err
	}
	message.From = normalizedFrom

	normalizedTarget, isDM, err := NormalizeTarget(message.To)
	if err != nil {
		return false, err
	}
	message.To = normalizedTarget

	if message.Time.IsZero() {
		return false, fmt.Errorf("missing time")
	}

	if err := message.Validate(); err != nil {
		return false, err
	}

	if err := s.EnsureRoot(); err != nil {
		return false, err
	}

	var dir string
	var filePerm os.FileMode
	if isDM {
		agent := strings.TrimPrefix(normalizedTarget, "@")
		dir, err = s.ensureDMDir(agent)
		if err != nil {
			return false, err
		}
		filePerm = dmFilePerm
	} else {
		dir = s.TopicDir(normalizedTarget)
		if err := ensureDirPerm(dir, topicDirPerm); err != nil {
			return false, err
		}
		filePerm = topicFilePerm
	}

	data, err := marshalMessage(message)
	if err != nil {
		return false, err
	}
	if len(data) > MaxMessageSize {
		return false, ErrMessageTooLarge
	}

	path := filepath.Join(dir, message.ID+".json")
	if err := writeFileExclusivePerm(path, data, filePerm); err != nil {
		if errors.Is(err, os.ErrExist) {
			return false, nil
		}
		return false, err
	}
	return true, nil
}

func (s *Store) ReadMessage(path string) (*Message, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	var msg Message
	if err := json.Unmarshal(data, &msg); err != nil {
		return nil, err
	}
	return &msg, nil
}

func (s *Store) ListTopicMessages(topic string) ([]Message, error) {
	normalized, err := NormalizeTopic(topic)
	if err != nil {
		return nil, err
	}
	return s.listMessages(s.TopicDir(normalized))
}

func (s *Store) ListDMMessages(agent string) ([]Message, error) {
	normalized, err := NormalizeAgentName(agent)
	if err != nil {
		return nil, err
	}
	return s.listMessages(s.DMDir(normalized))
}

func (s *Store) EnsureProject(id string) (*Project, error) {
	if err := s.EnsureRoot(); err != nil {
		return nil, err
	}
	path := s.ProjectFile()
	if _, err := os.Stat(path); err == nil {
		return readProject(path)
	}
	project := Project{ID: id, Created: s.now()}
	data, err := json.MarshalIndent(project, "", "  ")
	if err != nil {
		return nil, err
	}
	if err := writeFileExclusive(path, data); err != nil {
		if errors.Is(err, os.ErrExist) {
			return readProject(path)
		}
		return nil, err
	}
	return &project, nil
}

func readProject(path string) (*Project, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	var project Project
	if err := json.Unmarshal(data, &project); err != nil {
		return nil, err
	}
	return &project, nil
}

func (s *Store) listMessages(dir string) ([]Message, error) {
	entries, err := os.ReadDir(dir)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return nil, nil
		}
		return nil, err
	}

	names := make([]string, 0, len(entries))
	for _, entry := range entries {
		if entry.IsDir() {
			continue
		}
		if filepath.Ext(entry.Name()) != ".json" {
			continue
		}
		names = append(names, entry.Name())
	}
	sort.Strings(names)

	messages := make([]Message, 0, len(names))
	for _, name := range names {
		msg, err := s.ReadMessage(filepath.Join(dir, name))
		if err != nil {
			return nil, err
		}
		messages = append(messages, *msg)
	}
	return messages, nil
}

func writeFileExclusive(path string, data []byte) error {
	return writeFileExclusivePerm(path, data, topicFilePerm)
}

func writeFileExclusivePerm(path string, data []byte, perm os.FileMode) error {
	file, err := os.OpenFile(path, os.O_WRONLY|os.O_CREATE|os.O_EXCL, perm)
	if err != nil {
		return err
	}
	defer file.Close()

	if _, err := file.Write(data); err != nil {
		return err
	}
	return file.Close()
}

func ensureDirPerm(path string, perm os.FileMode) error {
	if err := os.MkdirAll(path, perm); err != nil {
		return err
	}
	if err := os.Chmod(path, perm); err != nil && !errors.Is(err, os.ErrPermission) {
		return err
	}
	return nil
}

func (s *Store) ensureDMDir(agent string) (string, error) {
	dmRoot := filepath.Join(s.Root, "dm")
	if err := ensureDirPerm(dmRoot, dmDirPerm); err != nil {
		return "", err
	}
	dir := filepath.Join(dmRoot, agent)
	if err := ensureDirPerm(dir, dmDirPerm); err != nil {
		return "", err
	}
	return dir, nil
}
