// xdg_foreign handler -> permite que apps establezcan relaciones padre-hijo
// entre ventanas de distintos procesos (ej: un file picker y su app padre)

// init del proceso

use crate::state::Vinland;
use smithay::wayland::xdg_foreign::{XdgForeignHandler, XdgForeignState};

impl XdgForeignHandler for Vinland {
    fn xdg_foreign_state(&mut self) -> &mut XdgForeignState {
        &mut self.xdg_foreign_state
    }
}

smithay::delegate_xdg_foreign!(Vinland);
