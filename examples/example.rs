// Usage example of egui-sdl2-renderer
// Based on examples of egui and egui_sdl2_platform

use std::thread;
use std::time::{Duration, Instant};

use egui_sdl2_platform::Platform;
use egui_sdl2_renderer::Painter;
use sdl2::event::{Event, WindowEvent};
use sdl2::rect::FRect;
use sdl2::render::BlendMode;

// 60 fps
const FRAME_TIME_TARGET: Duration = Duration::from_nanos(1_000_000_000 / 60);

fn main() {
    // The usual lines of code to init SDL2 video subsystem, canvas and event pump
    let sdl = sdl2::init().expect("unable to init sdl2");
    let mut video = sdl.video().expect("unable to init sdl2 video subsystem");
    let window = video
        .window("Window", 800, 600)
        .position_centered()
        .build()
        .expect("unable build window");
    let mut canvas = window.clone().into_canvas().build().unwrap();
    let mut event_pump = sdl.event_pump().unwrap();

    // Create Painter, this handles drawing egui using the SDL2 Renderer API
    let texture_creator = canvas.texture_creator();
    let mut painter = Painter::new(&texture_creator);

    // Create egui_sdl2_platform::Platform, this handles egui and events from SDL2
    // TODO .unwrap() should not be needed here
    let mut platform = Platform::new(window.size()).unwrap();

    // Parameters the user can change in this example
    let mut bg_color = [0.25; 3];
    let mut square_color = [0.0, 1.0, 0.0, 1.0];
    let mut square_size = 200.0;

    let start_time = Instant::now();
    let mut prev_frame_time = start_time;
    'main: loop {
        // Update the time
        platform.update_time(start_time.elapsed().as_secs_f64());

        // egui example
        egui::Window::new("egui-sdl2-renderer example").show(&platform.context(), |ui| {
            ui.heading("Hello, world!");

            ui.horizontal(|ui| {
                ui.label("Background color (RGB): ");
                ui.color_edit_button_rgb(&mut bg_color);
            });

            ui.horizontal(|ui| {
                ui.label("Square size: ");
                ui.add(egui::Slider::new(&mut square_size, 10.0..=500.0).text("pixels"));
            });
            ui.horizontal(|ui| {
                ui.label("Square color (RGBA): ");
                ui.color_edit_button_rgba_unmultiplied(&mut square_color);
            });
        });

        // Get egui::FullOutput after frame
        let full_output = platform
            .end_frame(&mut video)
            .expect("platform.end_frame error");
        // Tessellate
        let paint_jobs = platform.tessellate(&full_output);

        // Any other SDL2 rendering that needs to be done
        canvas.set_draw_color({
            let [r, g, b] = bg_color.map(|c| (c * 255.0) as u8);
            (r, g, b)
        });
        canvas.clear();

        // Enable alpha blending
        canvas.set_blend_mode(BlendMode::Blend);
        canvas.set_draw_color({
            let [r, g, b, a] = square_color.map(|c| (c * 255.0) as u8);
            (r, g, b, a)
        });
        canvas
            .fill_frect(FRect::from((10.0, 10.0, square_size, square_size)))
            .unwrap();

        // Paint egui
        let size = window.size();
        painter
            .paint_and_update_textures(
                &mut canvas,
                [size.0, size.1],
                1.0,
                &paint_jobs,
                &full_output.textures_delta,
            )
            .expect("painter.paint_and_update_textures error");

        // Present to the user
        canvas.present();

        // Get time, update prev_frame_time and sleep to target FRAME_TIME_TARGET
        let t = Instant::now();
        let elapsed = t - prev_frame_time;
        prev_frame_time = t;
        thread::sleep(FRAME_TIME_TARGET.saturating_sub(elapsed));

        // Handle sdl events
        for event in event_pump.poll_iter() {
            // Handle sdl events
            match event {
                Event::Window {
                    window_id,
                    win_event,
                    ..
                } => {
                    if window_id == window.id() && win_event == WindowEvent::Close {
                        break 'main;
                    }
                }
                Event::KeyDown {
                    keycode: Some(sdl2::keyboard::Keycode::Escape),
                    ..
                } => break 'main,
                _ => {}
            }
            // Let the egui platform handle the event
            platform.handle_event(&event, &sdl, &video);
        }
    }
}
