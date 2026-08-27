//   - compositorhandler -> wl_surface commits
//   - shmhandler -> buffers de memoria compartida (wl_shm)
//   - bufferhandler -> buffer destruido por el cliente

use smithay::wayland::compositor::{CompositorHandler, CompositorState, CompositorClientState};
use smithay::wayland::shm::ShmHandler;
use smithay::wayland::buffer::BufferHandler;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::Resource;
use smithay::backend::renderer::utils::on_commit_buffer_handler;

use crate::state::{Vinland, ClientState};

use smithay::xwayland::XWaylandClientData;

// compositorhandler -> wl_surface commits
impl CompositorHandler for Vinland {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    // devuelve el estado del compositor para un cliente específico
    fn client_compositor_state<'a>(
        &self,
        client: &'a smithay::reexports::wayland_server::Client,
    ) -> &'a CompositorClientState {
        if let Some(state) = client.get_data::<XWaylandClientData>() {
            return &state.compositor_state;
        }
        if let Some(state) = client.get_data::<ClientState>() {
            return &state.compositor_state;
        }
        panic!("Tipo de datos de cliente desconocido: {:?}", client);
    }

    fn commit(&mut self, surface: &WlSurface) {
        let role = smithay::wayland::compositor::get_role(surface);
        tracing::info!("[COMMIT] surface {:?} role={:?}", surface.id(), role);
        // registra el buffer del cliente en el estado interno de Smithay
        // sin esto, render_elements_from_surface_tree no puede importar el buffer
        on_commit_buffer_handler::<Self>(surface);

        // PopupManager.commit() mapea el popup (lo mueve de unmapped → mapped).
        // Después buscamos el popup para enviar el configure inicial si aún no fue enviado.
        // Este es el patrón correcto de Anvil: new_popup solo trackea, el commit configura.
        self.popups.commit(surface);
        if let Some(popup) = self.popups.find_popup(surface) {
            match popup {
                smithay::desktop::PopupKind::Xdg(ref xdg_popup) => {
                    if !xdg_popup.is_initial_configure_sent() {
                        // NOTE: El configure inicial siempre está permitido.
                        xdg_popup.send_configure().expect("popup initial configure failed");
                        tracing::info!("[compositor] popup configure inicial enviado para {:?}", surface.id());
                    }
                }
                _ => {}
            }
        }

        let mut should_retile = false;

        // si una layer surface (como waybar) mando un commit, retilamos para aplicar su exclusive_zone
        if self.layer_surfaces.iter().any(|item| item.surface.wl_surface() == surface) {
            should_retile = true;
        }

        // re-evaluamos reglas para clientes que asignaron app_id/title despues de new_toplevel
        let (app_id, title) = smithay::wayland::compositor::with_states(surface, |states| {
            states
                .data_map
                .get::<smithay::wayland::shell::xdg::XdgToplevelSurfaceData>()
                .map(|data| {
                    let guard = data.lock().unwrap();
                    (guard.app_id.clone(), guard.title.clone())
                })
                .unwrap_or((None, None))
        });

        let matched_rule = self
            .config
            .rules
            .iter()
            .find(|r| r.matches(app_id.as_deref(), title.as_deref()))
            .cloned();
        let default_dialog_w = self.config.floating.dialog_width;
        let default_dialog_h = self.config.floating.dialog_height;
        let gap = self.config.tiling.gap;
        let out_size = self.backend.window_size();

        for w in self.windows_mut() {
            if w.surface.wl_surface() == surface && w.surface.parent().is_none() && !w.rules_evaluated {
                w.rules_evaluated = true;
                // si no estaba marcada como floating, verificar si las reglas ahora coinciden
                if !w.floating {
                    if let Some(ref rule) = matched_rule {
                        if rule.float == Some(true) {
                            w.floating = true;
                            // desactivar estados tiled ya que ahora es flotante
                            w.surface.with_pending_state(|s| {
                                s.states.unset(smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State::TiledTop);
                                s.states.unset(smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State::TiledBottom);
                                s.states.unset(smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State::TiledLeft);
                                s.states.unset(smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State::TiledRight);
                            });
                            let width: i32 = rule.size.map(|s| s[0]).unwrap_or(default_dialog_w);
                            let height: i32 = rule.size.map(|s| s[1]).unwrap_or(default_dialog_h);
                            let x: i32 = ((out_size.w - width) / 2).max(gap);
                            let y: i32 = ((out_size.h - height) / 2).max(gap);
                            w.rect = smithay::utils::Rectangle::new((x, y).into(), (width, height).into());
                            w.surface.with_pending_state(|s| {
                                s.size = Some(smithay::utils::Size::from((width, height)));
                            });
                            w.surface.send_configure();
                            should_retile = true;
                            break;
                        }
                    }
                }
                break;
            }
        }

        if should_retile {
            self.retile();
        }

        // pedimos redibujo para que el próximo frame muestre el contenido del cliente
        self.backend.window().request_redraw();
    }
}

// shmhandler -> buffers de memoria compartida (wl_shm)
impl ShmHandler for Vinland {
    fn shm_state(&self) -> &smithay::wayland::shm::ShmState {
        &self.shm_state
    }
}

// bufferhandler -> buffer destruido por el cliente
impl BufferHandler for Vinland {
    fn buffer_destroyed(
        &mut self,
        _buffer: &smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer,
    ) {
        // todo: liberar texturas importadas de este buffer.
    }
}
