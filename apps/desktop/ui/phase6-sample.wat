(module
	(import "host" "state_toggle" (func $state_toggle (param i32 i32)))
	(import "host" "state_set_bool" (func $state_set_bool (param i32 i32 i32)))
	(import "host" "command" (func $command (param i32 i32)))
	(import "host" "command_string" (func $command_string (param i32 i32 i32 i32 i32 i32)))

	(memory (export "memory") 1)

	(data (i32.const 0) "expanded")
	(data (i32.const 16) "project.save")
	(data (i32.const 32) "project.load")
	(data (i32.const 48) "tool.set_active")
	(data (i32.const 80) "tool")
	(data (i32.const 96) "brush")
	(data (i32.const 112) "eraser")

	(func (export "panel_init")
		i32.const 0
		i32.const 8
		i32.const 0
		call $state_set_bool)

	(func (export "panel_handle_toggle_expanded")
		i32.const 0
		i32.const 8
		call $state_toggle)

	(func (export "panel_handle_save_project")
		i32.const 16
		i32.const 12
		call $command)

	(func (export "panel_handle_load_project")
		i32.const 32
		i32.const 12
		call $command)

	(func (export "panel_handle_activate_brush")
		i32.const 48
		i32.const 15
		i32.const 80
		i32.const 4
		i32.const 96
		i32.const 5
		call $command_string)

	(func (export "panel_handle_activate_eraser")
		i32.const 48
		i32.const 15
		i32.const 80
		i32.const 4
		i32.const 112
		i32.const 6
		call $command_string))