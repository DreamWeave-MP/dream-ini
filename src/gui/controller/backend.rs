use super::ControllerAction;

#[derive(Debug, Default)]
pub(super) struct ControllerBackend;

impl ControllerBackend {
    pub(super) fn poll(&mut self) -> Vec<ControllerAction> {
        Vec::new()
    }
}
