use tracing::info;
use tracing_subscriber;
use calloop::EventLoop;
use smithay::reexports::wayland_server::Display;
// struct del estado (state.rs)
mod state;
use state::Vinland;
use state::ClientState;
use smithay::wayland::compositor::CompositorClientState;
use smithay::backend::renderer::Renderer;
use smithay::backend::renderer::Frame;


fn main() {
    // sistema de logs
    tracing_subscriber::fmt::init();
    info!("iniciando vinland...");


    // declara eventloop y display
    let mut event_loop: EventLoop<Vinland> = EventLoop::try_new()
        .expect("fallo al inicializar el event loop");

        // inicio del struc para definir inicializaciones
    let display: Display<Vinland> = Display::new().unwrap();
    let display_handle = display.handle();
    let compositor_state = smithay::wayland::compositor::CompositorState::new::<Vinland>(&display_handle);
    let shm_state = smithay::wayland::shm::ShmState::new::<Vinland>(&display_handle, vec![]);
    let (backend, mut winit_evt_loop) = smithay::backend::winit::init::<smithay::backend::renderer::gles::GlesRenderer>()
        .expect("fallo al inicializar el backend de winit");
     
    let mut state = Vinland {display_handle, compositor_state, shm_state, backend};

    info!("display wayland creado");

    //loop handler, conecta display al loop
    let loop_handle = event_loop.handle();
loop_handle
    .insert_source(
        smithay::reexports::calloop::generic::Generic::new(
            display,
            calloop::Interest::READ,
            calloop::Mode::Level,
        ),


        // event handler
        |_, _, _state| {
            Ok(calloop::PostAction::Continue)
        },
    )
    .unwrap();

    info!("display conectado al event loop");

        let socket = smithay::wayland::socket::ListeningSocketSource::new_auto().unwrap();
    let socket_name = socket.socket_name().to_os_string();
    info!("socket wayland creado: {:?}", socket_name);

    loop_handle // para aceptar conexiones entrantes
        .insert_source(socket, |client_stream, _, state| {
            state
                .display_handle
                .insert_client(client_stream, std::sync::Arc::new(ClientState {
                    compositor_state: CompositorClientState::default(),
                }))
                .unwrap();
        })
        .unwrap();

    loop_handle // para manejar eventos de winit
        .insert_source(winit_evt_loop, |event, _, state| {
            match event {
                smithay::backend::winit::WinitEvent::CloseRequested => {
                    info!("ventana cerrada");
                }
                smithay::backend::winit::WinitEvent::Redraw => {
                                        // 1= tamaño actual de la ventana
                    let size = state.backend.window_size();
                    // 2= el "daño" = área que vamos a redibujar (toda la ventana)
                    let damage = smithay::utils::Rectangle::new((0, 0).into(), size);
                    // 3= bind: prepara el renderer para dibujar
                    let (renderer, mut framebuffer) = state.backend.bind().unwrap();
                    // 4= render: inicia un frame con el tamaño y orientación
                    let mut frame = renderer.render(
                        &mut framebuffer,
                        size,
                        smithay::utils::Transform::Normal,
                    ).unwrap();
                    // 5. clear: pinta toda la pantalla de un color
                    frame.clear(
                        smithay::backend::renderer::Color32F::from([0.0, 1.0, 0.0, 1.0]),
                        &[damage],
                    ).unwrap();
                    // 6. finish: termina el frame
                    frame.finish().unwrap();
                    drop(framebuffer);
                    // 7. submit: manda el frame a la pantalla
                    state.backend.submit(None).unwrap();
                }
                _ => {}
            }
        })
        .unwrap();

    event_loop.run(None, &mut state, |_| {}).unwrap();
}