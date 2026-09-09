// ipc.rs
// socket unix nativo y protocolo de comunicacion json
// permite consultar ventanas, cambiar workspaces y controlar vinland externamente

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use smithay::reexports::wayland_server::Resource;
use smithay::utils::Serial;

use crate::state::{ScreenshotTarget, Vinland};

#[derive(Debug, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum IpcRequest {
    ListWindows,
    Focus { id: Option<u32>, app_id: Option<String> },
    Close { id: u32 },
    Workspace { num: usize },
    MoveToWorkspace { id: u32, num: usize },
    ReloadConfig,
    GetConfig,
    Screenshot,
}

#[derive(Debug, Serialize)]
pub struct IpcWindowInfo {
    pub id: u32,
    pub workspace: usize,
    pub title: Option<String>,
    pub app_id: Option<String>,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub minimized: bool,
    pub floating: bool,
    pub focused: bool,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum IpcResponse {
    Ok { ok: bool, #[serde(skip_serializing_if = "Option::is_none")] message: Option<String> },
    Windows { ok: bool, windows: Vec<IpcWindowInfo> },
    Config { ok: bool, toml: String },
    Error { ok: bool, error: String },
}

/// calcula la ruta al socket ipc de vinland
pub fn ipc_socket_path(wayland_display: Option<&str>) -> PathBuf {
    let base_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    let display_name = wayland_display.unwrap_or("wayland-0");
    PathBuf::from(base_dir).join(format!("vinland-{}.sock", display_name))
}

/// inicializa el socket unix para el servidor ipc
pub fn init_ipc_listener(wayland_display: Option<&str>) -> std::io::Result<(UnixListener, PathBuf)> {
    let path = ipc_socket_path(wayland_display);
    if path.exists() {
        let _ = std::fs::remove_file(&path);
    }

    let listener = UnixListener::bind(&path)?;
    listener.set_nonblocking(true)?;
    tracing::info!("[ipc] socket inicializado en {:?}", path);
    Ok((listener, path))
}

/// procesa una linea de comando json y devuelve la respuesta en json
pub fn process_ipc_command(state: &mut Vinland, line: &str) -> String {
    let req: Result<IpcRequest, _> = serde_json::from_str(line.trim());
    let resp = match req {
        Ok(IpcRequest::ListWindows) => {
            let kb = state.seat.get_keyboard();
            let current_focus = kb.as_ref().and_then(|k| k.current_focus());
            let mut list = Vec::new();

            for (ws_idx, ws) in state.workspaces.iter().enumerate() {
                for win in &ws.windows {
                    let surf = win.surface.wl_surface();
                    let id = surf.id().protocol_id();
                    let focused = current_focus.as_ref().map(|f| f == surf).unwrap_or(false);

                    let (app_id, title) = smithay::wayland::compositor::with_states(surf, |states| {
                        states
                            .data_map
                            .get::<smithay::wayland::shell::xdg::XdgToplevelSurfaceData>()
                            .map(|data| {
                                let guard = data.lock().unwrap();
                                (guard.app_id.clone(), guard.title.clone())
                            })
                            .unwrap_or((None, None))
                    });

                    list.push(IpcWindowInfo {
                        id,
                        workspace: ws_idx + 1,
                        title,
                        app_id,
                        x: win.rect.loc.x,
                        y: win.rect.loc.y,
                        width: win.rect.size.w,
                        height: win.rect.size.h,
                        minimized: win.minimized,
                        floating: win.floating,
                        focused,
                    });
                }
            }

            IpcResponse::Windows { ok: true, windows: list }
        }
        Ok(IpcRequest::Focus { id, app_id }) => {
            let mut target_found = None;
            'outer: for (ws_idx, ws) in state.workspaces.iter().enumerate() {
                for win in &ws.windows {
                    let surf = win.surface.wl_surface();
                    let matches_id = id.map(|i| surf.id().protocol_id() == i).unwrap_or(false);
                    let matches_app = app_id.as_ref().map(|target_app| {
                        smithay::wayland::compositor::with_states(surf, |states| {
                            states
                                .data_map
                                .get::<smithay::wayland::shell::xdg::XdgToplevelSurfaceData>()
                                .and_then(|data| data.lock().unwrap().app_id.clone())
                                .map(|a| a.to_lowercase().contains(&target_app.to_lowercase()))
                                .unwrap_or(false)
                        })
                    }).unwrap_or(false);

                    if matches_id || matches_app {
                        target_found = Some((ws_idx, surf.clone()));
                        break 'outer;
                    }
                }
            }

            if let Some((ws_idx, surf)) = target_found {
                if ws_idx != state.active_workspace {
                    state.switch_workspace(ws_idx);
                }
                state.set_keyboard_focus_surface(Some(&surf), Serial::from(0));
                state.backend.window().request_redraw();
                IpcResponse::Ok { ok: true, message: Some(format!("foco asignado a ventana")) }
            } else {
                IpcResponse::Error { ok: false, error: "ventana no encontrada".to_string() }
            }
        }
        Ok(IpcRequest::Close { id }) => {
            let mut closed = false;
            for ws in &mut state.workspaces {
                for win in &ws.windows {
                    if win.surface.wl_surface().id().protocol_id() == id {
                        win.surface.send_close();
                        closed = true;
                        break;
                    }
                }
            }
            if closed {
                IpcResponse::Ok { ok: true, message: Some(format!("senal de cierre enviada a ventana {}", id)) }
            } else {
                IpcResponse::Error { ok: false, error: format!("ventana {} no encontrada", id) }
            }
        }
        Ok(IpcRequest::Workspace { num }) => {
            if num >= 1 && num <= state.workspaces.len() {
                state.switch_workspace(num - 1);
                IpcResponse::Ok { ok: true, message: Some(format!("cambiado al workspace {}", num)) }
            } else {
                IpcResponse::Error { ok: false, error: format!("numero de workspace invalido: {}", num) }
            }
        }
        Ok(IpcRequest::MoveToWorkspace { id, num }) => {
            if num < 1 || num > state.workspaces.len() {
                return serde_json::to_string(&IpcResponse::Error { ok: false, error: format!("workspace invalido: {}", num) }).unwrap() + "\n";
            }
            let target_ws = num - 1;
            let mut found = None;

            for (ws_idx, ws) in state.workspaces.iter().enumerate() {
                if let Some(pos) = ws.windows.iter().position(|w| w.surface.wl_surface().id().protocol_id() == id) {
                    found = Some((ws_idx, pos));
                    break;
                }
            }

            if let Some((src_ws, pos)) = found {
                if src_ws != target_ws {
                    let win = state.workspaces[src_ws].windows.remove(pos);
                    state.workspaces[target_ws].windows.push(win);
                    state.retile();
                    state.backend.window().request_redraw();
                }
                IpcResponse::Ok { ok: true, message: Some(format!("ventana {} movida al workspace {}", id, num)) }
            } else {
                IpcResponse::Error { ok: false, error: format!("ventana {} no encontrada", id) }
            }
        }
        Ok(IpcRequest::ReloadConfig) => {
            let new_config = crate::config::load();
            state.reload_config(new_config);
            IpcResponse::Ok { ok: true, message: Some("configuracion recargada".to_string()) }
        }
        Ok(IpcRequest::GetConfig) => {
            let path = crate::config::config_path();
            let toml_str = std::fs::read_to_string(&path).unwrap_or_default();
            IpcResponse::Config { ok: true, toml: toml_str }
        }
        Ok(IpcRequest::Screenshot) => {
            state.pending_screenshot = Some(ScreenshotTarget::FullScreen);
            state.backend.window().request_redraw();
            IpcResponse::Ok { ok: true, message: Some("captura de pantalla solicitada".to_string()) }
        }
        Err(e) => {
            IpcResponse::Error { ok: false, error: format!("error parseando json: {}", e) }
        }
    };

    serde_json::to_string(&resp).unwrap_or_else(|_| "{\"ok\":false,\"error\":\"error serializando respuesta\"}".to_string()) + "\n"
}

/// atiende clientes entrantes en el listener del socket unix
pub fn handle_ipc_connections(listener: &UnixListener, state: &mut Vinland) {
    while let Ok((stream, _)) = listener.accept() {
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut line = String::new();
        if reader.read_line(&mut line).is_ok() && !line.is_empty() {
            let reply = process_ipc_command(state, &line);
            let mut writer = stream;
            let _ = writer.write_all(reply.as_bytes());
            let _ = writer.flush();
        }
    }
}
