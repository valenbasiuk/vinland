// xdg shell handler -> xdg_wm_base, xdg_surface, xdg_toplevel
// extension de surface q da  propiedades de titulos, estados y arrastre

use smithay::reexports::wayland_server::protocol::wl_seat;
use smithay::utils::{Rectangle, Serial, Size, SERIAL_COUNTER};
use smithay::wayland::shell::xdg::{
    PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
};

use smithay::desktop::{find_popup_root_surface, PopupKind};
use smithay::input::Seat;
use smithay::reexports::wayland_server::Resource;
use tracing::{info, warn};

use crate::state::{Vinland, Window};

impl XdgShellHandler for Vinland {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    // llamado cuando un cliente crea una nueva ventana toplevel
    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        info!("nueva ventana: {:?}", surface.wl_surface().id());

        // si tiene padre -> floating
        if surface.parent().is_some() {
            // calculamos la posición centrada
            let out_size = self.backend.window_size();
            let dialog_w = 600;
            let dialog_h = 500;
            let x = (out_size.w - dialog_w) / 2;
            let y = (out_size.h - dialog_h) / 2;

            let rect = Rectangle::new((x, y).into(), (dialog_w, dialog_h).into());
            self.windows.push(Window {
                surface: surface.clone(),
                rect,
                minimized: false,
            });

            surface.with_pending_state(|s| {
                s.size = Some(Size::from((dialog_w, dialog_h)));
            });
            surface.send_configure();
        } else {
            // ventana normal -> la agregamos con rect cero temporalmente
            // se recalcula el tiling cuando haga su primer commit con buffer
            self.windows.push(Window {
                surface: surface.clone(),
                rect: Rectangle::new((0, 0).into(), (0, 0).into()),
                minimized: false,
            });
        }

        // foco de teclado a la ventana recien abierta
        let serial = SERIAL_COUNTER.next_serial();
        let wl_surface = self.windows.last().unwrap().surface.wl_surface().clone();
        let keyboard = self.seat.get_keyboard().unwrap();
        keyboard.set_focus(self, Some(wl_surface), serial);
    }

    // llamado por xdg_foreign cuando se establece una relación padre-hijo
    // post-creación (ej: un file dialog que se abre desde otra app via portal)
    fn parent_changed(&mut self, surface: ToplevelSurface) {
        let out_size = self.backend.window_size();
        let dialog_w = 600;
        let dialog_h = 500;
        let x = (out_size.w - dialog_w) / 2;
        let y = (out_size.h - dialog_h) / 2;

        // buscamos la ventana en nuestra lista (fue agregada con rect 0 por new_toplevel)
        // y la reposicionamos como floating centrada ahora que sabemos que tiene padre
        if let Some(win) = self
            .windows
            .iter_mut()
            .find(|w| w.surface.wl_surface() == surface.wl_surface())
        {
            win.rect = Rectangle::new((x, y).into(), (dialog_w, dialog_h).into());
            surface.with_pending_state(|s| {
                s.size = Some(Size::from((dialog_w, dialog_h)));
            });
            surface.send_configure();
        }
    }

    // llamado cuando una ventana toplevel es cerrada por el cliente
    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        self.windows
            .retain(|w| w.surface.wl_surface() != surface.wl_surface());
        info!("ventana cerrada por el cliente");
        // retile() redistribuye el espacio entre las ventanas restantes
        self.retile();
    }

    // llamado cuando el cliente solicita minimizar la ventana
    fn minimize_request(&mut self, surface: ToplevelSurface) {
        info!("solicitud de minimizar ventana");
        if let Some(win) = self
            .windows
            .iter_mut()
            .find(|w| w.surface.wl_surface() == surface.wl_surface())
        {
            win.minimized = true;
            self.retile();
        }
    }

    // popups: menús contextuales, dropdowns, tooltips
    //
    // El PositionerState le dice al compositor cómo colocar el popup:
    //   - anchor_rect: el rect del padre donde "anclar" el popup
    //   - anchor:      qué esquina/borde del anchor_rect usar como punto fijo
    //   - gravity:     en qué dirección "cae" el popup desde ese punto
    //   - offset:      desplazamiento adicional en X/Y
    //   - size:        el tamaño del popup (lo define la app)
    //
    // Nuestro trabajo: encontrar la posición en pantalla del padre,
    // sumarle la geometría que calcula el posicionador, y mandar configure.
    fn new_popup(&mut self, surface: PopupSurface, positioner: PositionerState) {
        // Guardamos la geometría calculada por el posicionador en la superficie
        surface.with_pending_state(|state| {
            state.geometry = positioner.get_geometry();
            state.positioner = positioner;
        });

        // Le pasamos el popup a PopupManager de Smithay
        if let Err(err) = self.popups.track_popup(PopupKind::from(surface)) {
            warn!("Error al registrar popup en PopupManager: {}", err);
        }
    }

    // grab: la app pide que el popup capture todo el input (menus desplegables)
    fn grab(&mut self, surface: PopupSurface, seat: wl_seat::WlSeat, serial: Serial) {
        let seat: Seat<Vinland> = Seat::from_resource(&seat).unwrap();
        let kind = PopupKind::Xdg(surface);
        if let Ok(root) = find_popup_root_surface(&kind) {
            let _ = self.popups.grab_popup(root, kind, &seat, serial);
        }
    }

    // reposition: la app quiere mover un popup ya existente
    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        surface.with_pending_state(|state| {
            state.geometry = positioner.get_geometry();
            state.positioner = positioner;
        });
        surface.send_repositioned(token);
        let _ = surface.send_configure();
    }
}
