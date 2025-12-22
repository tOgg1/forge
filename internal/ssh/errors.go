package ssh

import "errors"

var (
	ErrPassphraseRequired  = errors.New("passphrase required for private key")
	ErrSSHAgentUnavailable = errors.New("ssh agent not available")
)
