extern crate caper;

use caper::game::*;
use caper::imgui::Ui;
use caper::input::Key;
use caper::mesh::get_pos_perlin;
use caper::types::{DefaultTag, RenderItemBuilder, TransformBuilder};
use caper::utils::handle_fp_inputs;
use caper::utils::load_wavefront;

fn main() {
    // create an instance of Game
    let (mut game, event_loop) = Game::<DefaultTag>::new();

    // generate the instance positions
    let map_size = 50f32;
    let transforms = (0..2500)
        .map(|i| {
            let pos = (
                (i as f32 % map_size) * 2f32,
                ((i / map_size as i32) * 2) as f32,
            );
            let size = get_pos_perlin((pos.0, pos.1)) * 2f32;
            TransformBuilder::default()
                .pos((pos.0 * 5f32, size, pos.1 * 5f32))
                .scale((4.2f32, size, 4.2f32))
                .build()
                .unwrap()
        })
        .collect::<Vec<_>>();

    // add a render item to the game
    game.add_render_item(
        RenderItemBuilder::default()
            .vertices(load_wavefront(include_bytes!("assets/cube.obj")))
            .instance_transforms(transforms)
            .build()
            .unwrap(),
    );

    event_loop.run(move |event, _, _control_flow| {
        // run the engine update
        game.update(
            |_: &Ui| {},
            |g: &mut Game<DefaultTag>| -> UpdateStatus {
                // update the first person inputs
                handle_fp_inputs(&mut g.input, &mut g.cams[0]);

                // quit
                if g.input.keys_down.contains(&Key::Escape) {
                    return UpdateStatus::Finish;
                }

                UpdateStatus::Continue
            },
            event,
        );
    });
}
