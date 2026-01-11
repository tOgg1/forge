package fmail

import "strings"

func ensureDMReadAccess(runtime *Runtime, target watchTarget, allowOther bool, action string) error {
	if target.mode != watchDM {
		return nil
	}
	if allowOther {
		return nil
	}
	if runtime == nil {
		return Exitf(ExitCodeFailure, "runtime unavailable")
	}
	if strings.EqualFold(target.name, runtime.Agent) {
		return nil
	}
	if action == "" {
		action = "read"
	}
	return Exitf(ExitCodeFailure, "refusing to %s other agent DM inbox (@%s); use --allow-other-dm to override", action, target.name)
}
