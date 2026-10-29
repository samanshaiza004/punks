package main

import "core:testing"

@(test)
test_dialog_initial_location_modes :: proc(t: ^testing.T) {
	ordinary := `C:\Samples\Drums`
	testing.expect_value(t, dialog_initial_location_for_mode("none", ordinary), "")
	testing.expect_value(t, dialog_initial_location_for_mode("ordinary", ordinary), ordinary)
	testing.expect_value(t, dialog_initial_location_for_mode("extended", ordinary), `\\?\C:\Samples\Drums`)
	testing.expect_value(t, dialog_initial_location_for_mode("extended", `\\server\share\Samples`), `\\?\UNC\server\share\Samples`)
	already_extended := `\\?\C:\Samples`
	testing.expect_value(t, dialog_initial_location_for_mode("extended", already_extended), already_extended)
}
