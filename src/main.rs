// main.rs
// punto de entrada
// inicialización -> state.rs (Vinland::new)
// renderizado    -> render.rs
// protocolos     -> handlers/

use calloop::EventLoop;
use smithay::backend::winit::WinitInput;
use smithay::reexports::wayland_server::Display;
use smithay::wayland::compositor::CompositorClientState;
use std::time::Instant;
use tracing::info;

mod config;
mod cursor;
mod handlers;
mod render;
mod state;

use state::{ClientState, Vinland};

fn main() {
    tracing_subscriber::fmt::init();
    info!("iniciando vinland...");

    let start_time = Instant::now();
    let mut event_loop: EventLoop<Vinland> = EventLoop::try_new().unwrap();
    let display: Display<Vinland> = Display::new().unwrap();

    let config = config::load();

    // vinland::new -> inicializa todos los protocolos wayland y el backend
    // &display: display queda en main para pasarlo al generic source de calloop
    let (mut state, winit_evt_loop) = Vinland::new(&display, event_loop.get_signal(), config);
    info!("compositor inicializado");

    // cargar wallpaper (si está configurado) ahora que el renderer ya está listo
    {
        let (renderer, _fb) = state.backend.bind().expect("bind para wallpaper");
        state.wallpaper_texture = state::load_wallpaper(renderer, &state.config);
    }

    let loop_handle = event_loop.handle();

    // fuente 1 -> display wayland (mensajes entrantes de clientes)
    // generic wrappea el fd del display para que calloop lo monitoree
    // unsafe permite acceder al display (que ahora es de calloop)
    loop_handle
        .insert_source(
            smithay::reexports::calloop::generic::Generic::new(
                display,
                calloop::Interest::READ,
                calloop::Mode::Level,
            ),
            |_, display, state| {
                unsafe { display.get_mut().dispatch_clients(state).unwrap() };
                Ok(calloop::PostAction::Continue)
            },
        )
        .unwrap();

    // fuente 2 -> socket wayland (conexiones nuevas de clientes)
    let socket = smithay::wayland::socket::ListeningSocketSource::new_auto().unwrap();
    info!("socket wayland: {:?}", socket.socket_name());
    loop_handle
        .insert_source(socket, |stream, _, state| {
            state
                .display_handle
                .insert_client(
                    stream,
                    std::sync::Arc::new(ClientState {
                        compositor_state: CompositorClientState::default(),
                    }),
                )
                .unwrap();
        })
        .unwrap();

    // fuente 3 -> xwayland (socket x11)
    use smithay::xwayland::{X11Wm, XWayland, XWaylandEvent};
    use std::process::Stdio;

    let (xwayland, client) = XWayland::spawn(
        &state.display_handle,
        None,
        std::iter::empty::<(String, String)>(),
        std::iter::empty::<String>(),
        true,
        Stdio::null(),
        Stdio::null(),
        |_| (),
    )
    .expect("falló al iniciar XWayland");

    let dh = state.display_handle.clone();
    let handle = loop_handle.clone();
    loop_handle
        .insert_source(xwayland, move |event, _, state| match event {
            XWaylandEvent::Ready {
                x11_socket,
                display_number,
            } => {
                info!("socket X11 listo! export DISPLAY=:{display_number}");
                let wm = X11Wm::start_wm(handle.clone(), &dh, x11_socket, client.clone())
                    .expect("falló al conectar X11 Window Manager");
                state.xwm = Some(wm);
                state.xdisplay = Some(display_number);
            }
            XWaylandEvent::Error => {
                tracing::warn!("XWayland falló en el arranque");
            }
        })
        .unwrap();

    // fuente 4 -> winit
    loop_handle
        .insert_source(winit_evt_loop, move |event, _, state| {
            match event {
                smithay::backend::winit::WinitEvent::CloseRequested => {
                    info!("ventana cerrada");
                    state.loop_signal.stop();
                }
                smithay::backend::winit::WinitEvent::Redraw => {
                    render::render_frame(state, start_time);
                }
                smithay::backend::winit::WinitEvent::Input(event) => {
                    // input event del backend -> traducido a protocolos wayland en process_input_event
                    state.process_input_event::<WinitInput>(event);
                    // pedimos redibujo para actualizar el cursor visual
                    state.backend.window().request_redraw();
                }
                _ => {}
            }
        })
        .unwrap();

    // fuente 5 -> hot-reload de config.toml (watcher con notify y calloop::channel)
    let (tx, rx) = calloop::channel::channel::<()>();
    let config_path = config::config_path();
    let config_dir = config_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    let _ = std::fs::create_dir_all(&config_dir);

    use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
    let tx_watcher = tx.clone();
    let _watcher: Option<RecommendedWatcher> = match RecommendedWatcher::new(
        move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                if event.kind.is_modify() || event.kind.is_create() {
                    let _ = tx_watcher.send(());
                }
            }
        },
        notify::Config::default(),
    ) {
        Ok(mut w) => {
            if let Err(e) = w.watch(&config_dir, RecursiveMode::NonRecursive) {
                tracing::warn!("[hot-reload] no se pudo monitorear {:?}: {}", config_dir, e);
                None
            } else {
                info!("[hot-reload] monitoreando cambios en {:?}", config_dir);
                Some(w)
            }
        }
        Err(e) => {
            tracing::warn!("[hot-reload] no se pudo inicializar watcher: {}", e);
            None
        }
    };

    loop_handle
        .insert_source(rx, |event, _, state| {
            if let calloop::channel::Event::Msg(()) = event {
                info!("[hot-reload] cambio detectado en config.toml, recargando...");
                match config::reload() {
                    Ok(new_cfg) => {
                        state.reload_config(new_cfg);
                    }
                    Err(e) => {
                        tracing::warn!("[hot-reload] error al recargar config: {}", e);
                    }
                }
            }
        })
        .unwrap();

    // idle callback -> flushea respuestas pendientes a todos los clientes
    event_loop
        .run(None, &mut state, |state| {
            state.display_handle.flush_clients().unwrap();
        })
        .unwrap();
}
