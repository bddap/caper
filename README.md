# Caper
[![crates.io version](https://img.shields.io/crates/v/caper.svg)](https://crates.io/crates/caper)
[![Build status](https://travis-ci.org/shockham/caper.svg?branch=master)](https://travis-ci.org/shockham/caper)
[![Documentation](https://docs.rs/caper/badge.svg)](https://docs.rs/caper)

Minimalist game framework using [rust](https://www.rust-lang.org/).
Currently has systems for:
- Rendering ([glium](https://github.com/tomaka/glium))
- Input ([winit](https://github.com/tomaka/winit) via [volition](https://github.com/shockham/volition))
- Physics ([nphysics](https://github.com/sebcrozet/nphysics))
- Audio ([rodio](https://github.com/tomaka/rodio) via [impose](https://github.com/shockham/impose))

[**Documentation**](https://docs.rs/caper)

## Setup
### Linux
Due to the crate alsa-sys being use for linux the following packages are required:
#### Debian/Ubuntu etc
`apt install libasound2-dev pkg-config`
#### Fedora/RHEL/CentOS
`dnf install alsa-lib-devel`

## Usage
[Example](https://github.com/shockham/caper/blob/master/examples/simple.rs) of a basis for a game:
```rust
extern crate caper;

use caper::game::*;
use caper::imgui::Ui;
use caper::input::Key;
use caper::mesh::gen_cube;
use caper::types::{DefaultTag, RenderItemBuilder, TransformBuilder};
use caper::utils::handle_fp_inputs;

fn main() {
    // crate an instance of the game struct
    let (mut game, event_loop) = Game::<DefaultTag>::new();

    // define some items to be rendered
    game.add_render_item(
        RenderItemBuilder::default()
            .vertices(gen_cube())
            .instance_transforms(vec![TransformBuilder::default()
                .pos((-0.5, 0.0, -5.0))
                .build()
                .unwrap()])
            .build()
            .unwrap(),
    );

    // run the engine update
    start_loop(event_loop, move |events| {
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
            events,
        )
    });
}
```

Check out the [examples](https://github.com/shockham/caper/tree/master/examples) and run with:
```
cargo run --example transforms
```

[License](https://github.com/shockham/caper/blob/master/LICENSE.md)
