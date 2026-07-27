// xdg shell handler -> xdg_wm_base, xdg_surface, xdg_toplevel
// extension de surface q da  propiedades de titulos, estados y arrastre

use smithay::reexports::wayland_server::protocol::wl_seat;
use smithay::utils::{Rectangle, Serial, Size, SERIAL_COUNTER};
use smithay::wayland::shell::xdg::{
    PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
};

use crate::state::{Popup, Vinland, Window};
use smithay::reexports::wayland_server::Resource;
use tracing::info;

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
        // get_geometry() aplica anchor + gravity + offset y devuelve el rect
        // del popup relativo al origen de la superficie padre
        let geo = positioner.get_geometry();

        // buscamos la posición en pantalla de la superficie padre del popup
        // el padre puede ser un toplevel o a su vez otro popup
        let parent_loc = surface
            .get_parent_surface()
            .and_then(|parent_wl| {
                // primero buscamos en ventanas toplevels
                self.windows
                    .iter()
                    .find(|w| *w.surface.wl_surface() == parent_wl)
                    .map(|w| w.rect.loc)
                    // si no, buscamos entre popups existentes (popup anidado)
                    .or_else(|| {
                        self.popups
                            .iter()
                            .find(|p| *p.surface.wl_surface() == parent_wl)
                            .map(|p| p.loc)
                    })
            })
            .unwrap_or((0, 0).into()); // fallback: origen si no encontramos al padre

        // posición final en pantalla = posición del padre + offset del posicionador
        let loc = parent_loc + geo.loc;

        // mandamos configure al popup con su geometría calculada
        // el popup necesita este configure para poder enviar su primer frame
        surface.with_pending_state(|s| {
            s.geometry = geo;
        });
        if surface.send_configure().is_ok() {
            self.popups.push(Popup { surface, loc });
        }
    }

    // grab: la app pide que el popup capture todo el input hasta que se cierre
    // (ej: si el usuario clickea fuera del menú, el menú debe cerrarse)
    // por ahora lo dejamos sin implementar — el popup igual aparece pero
    // sin grab exclusivo del puntero
    fn grab(&mut self, _surface: PopupSurface, _seat: wl_seat::WlSeat, _serial: Serial) {}

    // reposition: la app quiere mover un popup ya existente
    // (ej: un dropdown que se abre cerca del borde de la pantalla y necesita reajustarse)
    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        // recalculamos la geometría con el nuevo posicionador
        let geo = positioner.get_geometry();

        surface.with_pending_state(|s| {
            s.geometry = geo;
        });
        // send_repositioned avisa a la app que el reposicionamiento fue aceptado
        surface.send_repositioned(token);
        let _ = surface.send_configure();

        // actualizamos la posición almacenada si el popup ya existe en nuestro vec
        if let Some(popup) = self
            .popups
            .iter_mut()
            .find(|p| p.surface.wl_surface() == surface.wl_surface())
        {
            let parent_loc = surface
                .get_parent_surface()
                .and_then(|parent_wl| {
                    self.windows
                        .iter()
                        .find(|w| *w.surface.wl_surface() == parent_wl)
                        .map(|w| w.rect.loc)
                })
                .unwrap_or((0, 0).into());
            popup.loc = parent_loc + geo.loc;
        }
    }
}
