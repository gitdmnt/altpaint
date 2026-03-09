(module
	(import "host" "state_toggle" (func $state_toggle (param i32 i32)))
	(import "host" "state_set_bool" (func $state_set_bool (param i32 i32 i32)))
	(import "host" "state_set_string" (func $state_set_string (param i32 i32 i32 i32)))
	(import "host" "host_get_string_len" (func $host_get_string_len (param i32 i32) (result i32)))
	(import "host" "host_get_string_copy" (func $host_get_string_copy (param i32 i32 i32 i32)))
	(import "host" "command" (func $command (param i32 i32)))
	(import "host" "command_string" (func $command_string (param i32 i32 i32 i32 i32 i32)))

	(memory (export "memory") 1)

	(data (i32.const 0) "expanded")
	(data (i32.const 16) "document_title")
	(data (i32.const 32) "active_tool")
	(data (i32.const 48) "active_color")
	(data (i32.const 64) "document.title")
	(data (i32.const 96) "tool.active")
	(data (i32.const 112) "color.active")
	(data (i32.const 128) "project.save")
	(data (i32.const 144) "project.load")
	(data (i32.const 160) "tool.set_active")
	(data (i32.const 192) "tool")
	(data (i32.const 208) "brush")
	(data (i32.const 224) "eraser")

	(func (export "panel_init")
		i32.const 0
		i32.const 8
		i32.const 0
		call $state_set_bool)

	(func (export "panel_handle_sync_host")
		(local $len i32)
		i32.const 64
		i32.const 14
		call $host_get_string_len
		local.set $len
		i32.const 64
		i32.const 14
		i32.const 256
		local.get $len
		call $host_get_string_copy
		i32.const 16
		i32.const 14
		i32.const 256
		local.get $len
		call $state_set_string

		i32.const 96
		i32.const 11
		call $host_get_string_len
		local.set $len
		i32.const 96
		i32.const 11
		i32.const 320
		local.get $len
		call $host_get_string_copy
		i32.const 32
		i32.const 11
		i32.const 320
		local.get $len
		call $state_set_string

		i32.const 112
		i32.const 12
		call $host_get_string_len
		local.set $len
		i32.const 112
		i32.const 12
		i32.const 384
		local.get $len
		call $host_get_string_copy
		i32.const 48
		i32.const 12
		i32.const 384
		local.get $len
		call $state_set_string)

	(func (export "panel_handle_toggle_expanded")
		i32.const 0
		i32.const 8
		call $state_toggle)

	(func (export "panel_handle_save_project")
		i32.const 128
		i32.const 12
		call $command)

	(func (export "panel_handle_load_project")
		i32.const 144
		i32.const 12
		call $command)

	(func (export "panel_handle_activate_brush")
		i32.const 160
		i32.const 15
		i32.const 192
		i32.const 4
		i32.const 208
		i32.const 5
		call $command_string)

	(func (export "panel_handle_activate_eraser")
		i32.const 160
		i32.const 15
		i32.const 192
		i32.const 4
		i32.const 224
		i32.const 6
		call $command_string))
