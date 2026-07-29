// xdg shell handler -> xdg_wm_base, xdg_surface, xdg_toplevel
// extension de surface q da  propiedades de titulos, estados y arrastre

use smithay::reexports::wayland_server::protocol::wl_seat;
use smithay::utils::{Rectangle, Serial, Size, SERIAL_COUNTER};
use smithay::input::pointer::Focus;
use smithay::wayland::shell::xdg::{
    PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
};

use smithay::desktop::{
    find_popup_root_surface, PopupKeyboardGrab, PopupKind, PopupPointerGrab, PopupUngrabStrategy,
};
use smithay::input::Seat;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::reexports::wayland_server::Resource;
use tracing::info;

use crate::state::{Vinland, Window};

impl XdgShellHandler for Vinland {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    // llamado cuando un cliente crea una nueva ventana toplevel
    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        info!("nueva ventana: {:?}", surface.wl_surface().id());

        // si tiene padre -> ventana flotante / diálogo / menú desplegable hijo
        if surface.parent().is_some() {
            // No forzamos un tamaño fijo de 600x500; permitimos que el cliente especifique su propio tamaño
            let rect = Rectangle::new((100, 100).into(), (0, 0).into());
            self.windows.push(Window {
                surface: surface.clone(),
                rect,
                minimized: false,
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

        // Marcamos la ventana como Activated desde el principio.
        // GTK deshabilita las GActions (y botones vinculados a ellas, como el menú ≡)
        // si la ventana no está en estado Activated. Si no enviamos esto aquí,
        // la ventana nace inactiva y el primer click en ≡ siempre es ignorado por GTK.
        // Nota: para ventanas normales no enviamos configure aquí (lo hace retile())
        // pero sí para las que tienen padre.
        {
            let win = self.windows.last_mut().unwrap();
            win.surface.with_pending_state(|s| {
                s.states.set(xdg_toplevel::State::Activated);
            });
            // Para ventanas con padre se envía configure en new_toplevel (ya está arriba).
            // Para ventanas normales, retile() enviará el configure más adelante con el
            // tamaño correcto, y el estado Activated quedará incluido en ese configure.
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
        let geo = positioner.get_geometry();
        info!("[XDG] new_popup recibido para surface {:?} con geo {:?}", surface.wl_surface().id(), geo);

        // IMPORTANTE: NO llamar send_configure() aquí.
        // El configure inicial del xdg_popup DEBE enviarse en el primer commit
        // (cuando is_initial_configure_sent() == false), no en new_popup().
        // Enviarlo aquí puede causar una race condition con la inicialización
        // del xdg_surface del cliente, haciendo que el popup nunca se configure.
        surface.with_pending_state(|state| {
            state.geometry = geo;
            state.positioner = positioner;
        });

        if let Err(e) = self.popups.track_popup(PopupKind::from(surface)) {
            tracing::warn!("[XDG] new_popup: track_popup falló: {:?}", e);
        } else {
            info!("[XDG] new_popup: popup trackeado (configure se enviará en el primer commit)");
        }
    }

    // grab: la app pide que el popup capture todo el input (menús desplegables)
    fn grab(&mut self, surface: PopupSurface, seat: wl_seat::WlSeat, serial: Serial) {
        info!("[XDG] grab solicitado para popup {:?}", surface.wl_surface().id());
        let seat: Seat<Vinland> = Seat::from_resource(&seat).unwrap();
        let kind = PopupKind::Xdg(surface);
        if let Ok(root) = find_popup_root_surface(&kind) {
            let ret = self.popups.grab_popup(root, kind, &seat, serial);

            if let Ok(mut grab) = ret {
                if let Some(keyboard) = seat.get_keyboard() {
                    if keyboard.is_grabbed()
                        && !(keyboard.has_grab(serial)
                            || keyboard.has_grab(grab.previous_serial().unwrap_or(serial)))
                    {
                        grab.ungrab(PopupUngrabStrategy::All);
                        return;
                    }
                    keyboard.set_focus(self, grab.current_grab(), serial);
                    keyboard.set_grab(self, PopupKeyboardGrab::new(&grab), serial);
                }
                if let Some(pointer) = seat.get_pointer() {
                    if pointer.is_grabbed()
                        && !(pointer.has_grab(serial)
                            || pointer.has_grab(grab.previous_serial().unwrap_or_else(|| grab.serial())))
                    {
                        grab.ungrab(PopupUngrabStrategy::All);
                        return;
                    }
                    pointer.set_grab(self, PopupPointerGrab::new(&grab), serial, Focus::Keep);
                }
            }
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
