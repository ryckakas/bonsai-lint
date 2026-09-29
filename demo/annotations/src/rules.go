package engine

func appExitNonzeroRule(code int) int {
	if code > 0 {
		if code > 1 {
			if code > 2 {
				if code > 3 {
					if code > 4 {
						if code > 5 {
							if code > 6 {
								return code
							}
						}
					}
				}
			}
		}
	}
	return 0
}
