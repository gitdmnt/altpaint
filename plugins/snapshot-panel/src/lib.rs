#[panel_sdk::panel_init]
fn init() {}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn panel_init_is_callable() {
		init();
	}
}
