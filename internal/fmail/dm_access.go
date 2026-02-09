package fmail

// DMs are public; keep helper for compatibility with older call sites.
func ensureDMReadAccess(_ *Runtime, _ watchTarget, _ bool, _ string) error {
	return nil
}
