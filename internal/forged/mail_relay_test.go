package forged

import (
	"bufio"
	"context"
	"encoding/json"
	"net"
	"testing"
	"time"

	"github.com/rs/zerolog"
	"github.com/stretchr/testify/require"
	"github.com/tOgg1/forge/internal/fmail"
)

func TestMailRelayCrossHost(t *testing.T) {
	skipNetworkTest(t)

	rootA := t.TempDir()
	rootB := t.TempDir()
	projectID := "proj-relay-test"

	storeA, err := fmail.NewStore(rootA)
	require.NoError(t, err)
	require.NoError(t, storeA.EnsureRoot())
	_, err = storeA.EnsureProject(projectID)
	require.NoError(t, err)

	storeB, err := fmail.NewStore(rootB)
	require.NoError(t, err)
	require.NoError(t, storeB.EnsureRoot())
	_, err = storeB.EnsureProject(projectID)
	require.NoError(t, err)

	serverA := newMailServer(zerolog.Nop())
	resolverA, err := newStaticProjectResolver(rootA)
	require.NoError(t, err)

	listenerA, err := net.Listen("tcp", "127.0.0.1:0")
	require.NoError(t, err)
	defer listenerA.Close()

	go func() {
		_ = serverA.Serve(listenerA, resolverA, true)
	}()

	serverB := newMailServer(zerolog.Nop())
	relay := newMailRelayManager(zerolog.Nop(), serverB, serverB.host, []string{listenerA.Addr().String()}, 200*time.Millisecond, 100*time.Millisecond)

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	require.NoError(t, relay.Start(ctx, []mailProject{{ID: projectID, Root: rootB}}))
	defer relay.Stop()

	sendConn, err := net.Dial("tcp", listenerA.Addr().String())
	require.NoError(t, err)
	defer sendConn.Close()

	sendReq := mailSendRequest{
		mailBaseRequest: mailBaseRequest{
			Cmd:       "send",
			ProjectID: projectID,
			Agent:     "sender",
			ReqID:     "s1",
		},
		To:   "task",
		Body: json.RawMessage(`"hello"`),
	}
	writeLine(t, sendConn, sendReq)

	var sendResp mailResponse
	readJSONLine(t, bufio.NewReader(sendConn), &sendResp)
	require.True(t, sendResp.OK)
	require.NotEmpty(t, sendResp.ID)

	require.Eventually(t, func() bool {
		messages, err := storeB.ListTopicMessages("task")
		if err != nil || len(messages) != 1 {
			return false
		}
		return messages[0].ID == sendResp.ID && messages[0].Body == "hello"
	}, 2*time.Second, 50*time.Millisecond)
}
