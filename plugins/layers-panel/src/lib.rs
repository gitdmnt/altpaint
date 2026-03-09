#[unsafe(no_mangle)]
pub extern "C" fn panel_init() {}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn panel_init_is_callable() {
		panel_init();
	}
}
