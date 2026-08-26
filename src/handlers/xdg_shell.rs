// xdg shell handler -> xdg_wm_base, xdg_surface, xdg_toplevel
// extension de surface q da  propiedades de titulos, estados y arrastre

use smithay::reexports::wayland_server::protocol::wl_seat;
use smithay::utils::{Rectangle, Serial, Size, SERIAL_COUNTER};
use smithay::input::pointer::Focus;
use smithay::wayland::shell::xdg::{
    PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
};

use smithay::desktop::{
    find_popup_root_surface, get_popup_toplevel_coords, PopupKeyboardGrab, PopupKind,
    PopupPointerGrab, PopupUngrabStrategy,
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

        // consultar app_id y title iniciales si el cliente ya los asignó
        let (app_id, title) = smithay::wayland::compositor::with_states(surface.wl_surface(), |states| {
            states
                .data_map
                .get::<smithay::wayland::shell::xdg::XdgToplevelSurfaceData>()
                .map(|data| {
                    let guard = data.lock().unwrap();
                    (guard.app_id.clone(), guard.title.clone())
                })
                .unwrap_or((None, None))
        });

        // buscar si alguna regla en config coincide
        let matched_rule = self
            .config
            .rules
            .iter()
            .find(|r| r.matches(app_id.as_deref(), title.as_deref()))
            .cloned();

        let mut is_floating = surface.parent().is_some();
        let mut target_workspace = None;
        let mut custom_rect = None;

        if let Some(ref rule) = matched_rule {
            if let Some(float_pref) = rule.float {
                is_floating = float_pref;
            }
            if let Some(ws) = rule.workspace {
                if ws >= 1 && ws <= self.workspaces.len() {
                    target_workspace = Some(ws - 1);
                }
            }
            if is_floating {
                let w: i32 = rule
                    .size
                    .map(|s| s[0])
                    .unwrap_or(self.config.floating.dialog_width);
                let h: i32 = rule
                    .size
                    .map(|s| s[1])
                    .unwrap_or(self.config.floating.dialog_height);

                let out_size = self.backend.window_size();

                let should_center = rule.center.unwrap_or(true);
                let gap = self.config.tiling.gap;
                let x: i32 = if should_center {
                    ((out_size.w - w) / 2).max(gap)
                } else {
                    100
                };
                let y: i32 = if should_center {
                    ((out_size.h - h) / 2).max(gap)
                } else {
                    100
                };
                custom_rect = Some(Rectangle::new((x, y).into(), (w, h).into()));
            }
        }

        let rect = if let Some(r) = custom_rect {
            r
        } else if is_floating {
            Rectangle::new((100, 100).into(), (0, 0).into())
        } else {
            Rectangle::new((0, 0).into(), (0, 0).into())
        };

        let target_ws_idx = target_workspace.unwrap_or(self.active_workspace);

        let tile_order = self.workspaces[target_ws_idx].windows.len();
        self.workspaces[target_ws_idx].windows.push(Window {
            surface: surface.clone(),
            rect,
            minimized: false,
            floating: is_floating,
            tile_order,
        });

        if is_floating {
            let win = self.workspaces[target_ws_idx].windows.last_mut().unwrap();
            win.surface.with_pending_state(|s| {
                if let Some(r) = custom_rect {
                    s.size = Some(r.size);
                }
                s.states.set(xdg_toplevel::State::Activated);
            });
            win.surface.send_configure();
        } else {
            let win = self.workspaces[target_ws_idx].windows.last_mut().unwrap();
            win.surface.with_pending_state(|s| {
                s.states.set(xdg_toplevel::State::Activated);
                s.states.set(xdg_toplevel::State::TiledTop);
                s.states.set(xdg_toplevel::State::TiledBottom);
                s.states.set(xdg_toplevel::State::TiledLeft);
                s.states.set(xdg_toplevel::State::TiledRight);
            });
            if target_ws_idx == self.active_workspace {
                self.retile();
            } else {
                win.surface.send_configure();
            }
        }

        // foco de teclado a la ventana recien abierta solo si esta en el workspace activo
        if target_ws_idx == self.active_workspace {
            let serial = SERIAL_COUNTER.next_serial();
            let wl_surface = surface.wl_surface().clone();
            let keyboard = self.seat.get_keyboard().unwrap();
            keyboard.set_focus(self, Some(wl_surface), serial);
        }
    }

    // llamado por xdg_foreign cuando se establece una relación padre-hijo
    // post-creación (ej: un file dialog que se abre desde otra app via portal)
    fn parent_changed(&mut self, surface: ToplevelSurface) {
        let out_size = self.backend.window_size();
        let dialog_w = self.config.floating.dialog_width;
        let dialog_h = self.config.floating.dialog_height;
        let x = (out_size.w - dialog_w) / 2;
        let y = (out_size.h - dialog_h) / 2;

        // buscamos la ventana en nuestra lista y la marcamos como floating
        if let Some(win) = self
            .windows_mut()
            .iter_mut()
            .find(|w| w.surface.wl_surface() == surface.wl_surface())
        {
            win.floating = true;
            win.rect = Rectangle::new((x, y).into(), (dialog_w, dialog_h).into());
            surface.with_pending_state(|s| {
                s.size = Some(Size::from((dialog_w, dialog_h)));
            });
            surface.send_configure();
        }
    }

    // llamado cuando una ventana toplevel es cerrada por el cliente
    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        self.windows_mut()
            .retain(|w| w.surface.wl_surface() != surface.wl_surface());
        info!("ventana cerrada por el cliente");
        // retile() redistribuye el espacio entre las ventanas restantes
        self.retile();
    }

    // llamado cuando el cliente solicita minimizar la ventana
    fn minimize_request(&mut self, surface: ToplevelSurface) {
        info!("solicitud de minimizar ventana");
        if let Some(win) = self
            .windows_mut()
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

        self.unconstrain_popup(&surface);

        if let Err(e) = self.popups.track_popup(PopupKind::from(surface)) {
            tracing::warn!("[XDG] new_popup: track_popup falló: {:?}", e);
        } else {
            info!("[XDG] new_popup: popup trackeado (configure se enviará en el primer commit)");
        }
    }

    // grab: la app pide que el popup capture todo el input (menús desplegables)
    fn grab(&mut self, surface: PopupSurface, seat: wl_seat::WlSeat, serial: Serial) {
        info!("[XDG] grab solicitado para popup {:?}, serial: {:?}", surface.wl_surface().id(), serial);
        let seat: Seat<Vinland> = Seat::from_resource(&seat).unwrap();
        let kind = PopupKind::Xdg(surface);
        if let Ok(root) = find_popup_root_surface(&kind) {
            let ret = self.popups.grab_popup(root, kind, &seat, serial);

            match ret {
                Ok(mut grab) => {
                    info!("[XDG] grab_popup OK");
                    if let Some(keyboard) = seat.get_keyboard() {
                        if keyboard.is_grabbed()
                            && !(keyboard.has_grab(serial)
                                || keyboard.has_grab(grab.previous_serial().unwrap_or(serial)))
                        {
                            info!("[XDG] keyboard ya estaba grabado y serial no coincide -> UNGRAB!");
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
                            info!("[XDG] pointer ya estaba grabado y serial no coincide -> UNGRAB!");
                            grab.ungrab(PopupUngrabStrategy::All);
                            return;
                        }
                        pointer.set_grab(self, PopupPointerGrab::new(&grab), serial, Focus::Keep);
                    }
                }
                Err(err) => {
                    tracing::warn!("[XDG] grab_popup error: {:?}", err);
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
        self.unconstrain_popup(&surface);
        surface.send_repositioned(token);
        let _ = surface.send_configure();
    }
}

impl Vinland {
    pub fn unconstrain_popup(&self, popup: &PopupSurface) {
        let Ok(root) = find_popup_root_surface(&PopupKind::Xdg(popup.clone())) else {
            return;
        };
        let Some(window) = self.windows().iter().find(|w| w.surface.wl_surface() == &root) else {
            return;
        };

        let scale_factor = self.backend.scale_factor();
        let out_size = self
            .backend
            .window_size()
            .to_f64()
            .to_logical(scale_factor)
            .to_i32_round();

        let mut target = Rectangle::new((0, 0).into(), out_size);
        target.loc -= get_popup_toplevel_coords(&PopupKind::Xdg(popup.clone()));
        target.loc -= window.rect.loc;

        popup.with_pending_state(|state| {
            state.geometry = state.positioner.get_unconstrained_geometry(target);
        });
    }
}
