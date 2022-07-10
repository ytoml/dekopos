// TODO:
pub trait _ClassDriver {
    fn init();
    fn set_endpoint(&self);
    fn on_endpoints_completed(&self);
    fn on_interrupt_completed(&self);
    fn parent(&self);
}

pub trait _HidDriver: _ClassDriver {}

pub struct _KeyBoardDriver {}

pub struct _MouseDriver {}
