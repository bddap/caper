#[cfg(feature = "default")]
#[cfg(not(feature = "3d-audio"))]
use audio::Audio;
#[cfg(feature = "3d-audio")]
use audio::{Ambisonic, AmbisonicBuilder};
use imgui::Ui;
use input::Input;
use renderer::{Draw, Renderer};
use types::{Camera, PhysicsType, RenderItem, TextItem};

use nalgebra::Translation3;
use nalgebra::Vector3 as nVector3;
use ncollide::shape::{Cuboid, ShapeHandle};
use nphysics3d::object::{BodyHandle, BodyStatus, ColliderDesc, RigidBodyDesc};
use nphysics3d::world::World;

use glium::glutin::event::{Event, StartCause};
use glium::glutin::event_loop::{ControlFlow, EventLoop};

//use std::slice::IterMut;
use std::time::{Duration, Instant};

use rayon::prelude::*;
use rayon::slice::IterMut;

/// The divisor for the physics space to align with render space
const PHYSICS_DIVISOR: f32 = 2f32;

/// Enum for update to return status
pub enum UpdateStatus {
    /// Contune to update
    Continue,
    /// Finish/Exit the game
    Finish,
}

/// Struct for storing physics handles and associated RenderItem
struct PhysicsHandle {
    render_item: (usize, usize),
    body_handle: BodyHandle,
}

/// Struct for creating an instance of a game with all systems and items contained
pub struct Game<T: Default> {
    /// The input system for the game
    pub input: Input,
    /// The render system for the game
    pub renderer: Renderer,
    /// The physics system
    pub physics: World<f32>,
    /// The audio system
    #[cfg(feature = "default")]
    #[cfg(not(feature = "3d-audio"))]
    pub audio: Audio,
    /// The 3d audio system
    #[cfg(feature = "3d-audio")]
    pub audio: Ambisonic,
    /// Simple struct for camera data
    pub cams: Vec<Camera>,
    /// All of the mesh items to be rendered in the game
    render_items: Vec<RenderItem<T>>,
    /// All the text items to be rendered in the game
    text_items: Vec<TextItem>,
    /// All the body handles for physics items
    physics_items: Vec<PhysicsHandle>,
    /// The delta time for each frame
    pub delta: f32,
}

impl<T: Default> Game<T> {
    /// Creates a new instance of a game
    pub fn new() -> (Game<T>, EventLoop<()>) {
        // init physics
        let mut physics = World::new();
        physics.set_gravity(nVector3::new(0.0, -9.81, 0.0));

        //cam state
        let cam = Camera {
            pos: (0.0f32, 0.0, 0.0),
            euler_rot: (0.0f32, 0.0, 0.0),
        };

        let event_loop = EventLoop::new();

        let renderer = Renderer::new("caper window".to_string(), &event_loop);

        #[cfg(feature = "default")]
        #[cfg(not(feature = "3d-audio"))]
        let audio = Audio::new();
        #[cfg(feature = "3d-audio")]
        let audio = AmbisonicBuilder::default().build();

        (
            Game {
                input: Input::new(),
                renderer,
                physics,
                #[cfg(feature = "default")]
                #[cfg(feature = "3d-audio")]
                audio: audio,
                cams: vec![cam],
                render_items: Vec::new(),
                text_items: Vec::new(),
                physics_items: Vec::new(),
                delta: 0.016_666_667f32,
            },
            event_loop,
        )
    }
}

/// Default trait implementation from Game
impl<T: Default> Default for Game<T> {
    /// Returns a default instance of Game
    fn default() -> Self {
        Self::new().0
    }
}

/// Trait for operations on RenderItem
pub trait RenderItems {
    /// RenderItem utype associated type
    type T: Default;
    /// Get the len of render_items
    fn render_items_len(&self) -> usize;
    /// Get an IterMut of the RenderItem
    fn render_items_iter_mut(&mut self) -> IterMut<RenderItem<Self::T>>;
    /// Get a ref to a render item
    fn get_render_item(&mut self, index: usize) -> &mut RenderItem<Self::T>;
    /// Get a ref to a render item from its name, returning the first found
    fn get_render_item_by_name(&mut self, name: &str) -> Option<&mut RenderItem<Self::T>>;
    /// Add a render item to the game
    fn add_render_item(&mut self, render_item: RenderItem<Self::T>);
}

impl<T: Default> RenderItems for Game<T> {
    /// Associated type for RenderItems
    type T = T;
    /// Get the len of render_items
    fn render_items_len(&self) -> usize {
        self.render_items.len()
    }

    /// Get an IterMut of the RenderItem
    fn render_items_iter_mut(&mut self) -> IterMut<RenderItem<T>> {
        self.render_items.par_iter_mut()
    }

    /// Get a ref to a render item
    fn get_render_item(&mut self, index: usize) -> &mut RenderItem<T> {
        &mut self.render_items[index]
    }

    /// Get a ref to a render item from its name, returning the first found
    fn get_render_item_by_name(&mut self, name: &str) -> Option<&mut RenderItem<T>> {
        self.render_items.iter_mut().find(|item| item.name == name)
    }

    /// Add a render item to the game
    fn add_render_item(&mut self, render_item: RenderItem<T>) {
        // add the render item
        self.render_items.push(render_item);

        // the index of the newly added item
        let i = self.render_items.len() - 1;

        // setup the physics for the item
        self.add_physics(i);
    }
}

/// Trait for physics operations
pub trait Physics {
    /// Initalise physics depending on PhysicsType
    fn add_physics(&mut self, i: usize);
    /// Update physics
    fn update_physics(&mut self);
}

impl<T: Default> Physics for Game<T> {
    /// Initalise physics depending on PhysicsType
    fn add_physics(&mut self, i: usize) {
        // add the rigid body if needed
        match self.render_items[i].physics_type {
            PhysicsType::Static => {
                for j in 0..self.render_items[i].instance_transforms.len() {
                    let ri_trans = self.render_items[i].instance_transforms[j];

                    let geom = ShapeHandle::new(Cuboid::new(nVector3::new(
                        ri_trans.scale.0,
                        ri_trans.scale.1,
                        ri_trans.scale.2,
                    )));
                    let collider_desc = ColliderDesc::new(geom).density(1.0);

                    let mut rb_desc = RigidBodyDesc::new().collider(&collider_desc);

                    let pos = nVector3::new(
                        ri_trans.pos.0 * PHYSICS_DIVISOR,
                        ri_trans.pos.1 * PHYSICS_DIVISOR,
                        ri_trans.pos.2 * PHYSICS_DIVISOR,
                    );

                    let rb = rb_desc
                        .set_translation(pos)
                        .set_status(BodyStatus::Static)
                        .build(&mut self.physics);

                    let physics_handle = PhysicsHandle {
                        render_item: (i, j),
                        body_handle: rb.handle(),
                    };
                    self.physics_items.push(physics_handle);
                }
            }
            PhysicsType::Dynamic => {
                for j in 0..self.render_items[i].instance_transforms.len() {
                    let ri_trans = self.render_items[i].instance_transforms[j];

                    let geom = ShapeHandle::new(Cuboid::new(nVector3::new(
                        ri_trans.scale.0,
                        ri_trans.scale.1,
                        ri_trans.scale.2,
                    )));
                    let collider_desc = ColliderDesc::new(geom).density(1.0);

                    let mut rb_desc = RigidBodyDesc::new().collider(&collider_desc);

                    let pos = nVector3::new(
                        ri_trans.pos.0 * PHYSICS_DIVISOR,
                        ri_trans.pos.1 * PHYSICS_DIVISOR,
                        ri_trans.pos.2 * PHYSICS_DIVISOR,
                    );

                    let rb = rb_desc.set_translation(pos).build(&mut self.physics);

                    let physics_handle = PhysicsHandle {
                        render_item: (i, j),
                        body_handle: rb.handle(),
                    };
                    self.physics_items.push(physics_handle);
                }
            }
            PhysicsType::None => {}
        }
    }

    /// Update the physics engine
    fn update_physics(&mut self) {
        // update the new positions back to rb
        {
            for ph in self.physics_items.iter() {
                let (ri_i, ri_it_i) = ph.render_item;

                // check if it actually exists, if it doesn't remove
                if self.render_items.len() > ri_i
                    && self.render_items[ri_i].instance_transforms.len() > ri_it_i
                {
                    // update the rb transform pos
                    let rb = self.physics.rigid_body_mut(ph.body_handle).unwrap();
                    let ri_pos = self.render_items[ri_i].instance_transforms[ri_it_i].pos;

                    let mut rb_pos = rb.position().clone();
                    rb_pos.translation = Translation3::new(
                        ri_pos.0 * PHYSICS_DIVISOR,
                        ri_pos.1 * PHYSICS_DIVISOR,
                        ri_pos.2 * PHYSICS_DIVISOR,
                    );
                    rb.set_position(rb_pos);
                }
            }
        }

        // block for updating physics
        {
            // update all the physics items
            self.physics.step();

            for ph in self.physics_items.iter() {
                let rb = self.physics.rigid_body_mut(ph.body_handle).unwrap();

                // update the RenderItem transform pos
                let trans = rb.position().translation.vector;
                let prot = rb.position().rotation;
                let rot = prot.coords.data.as_slice();

                let (ri_i, ri_it_i) = ph.render_item;

                if self.render_items.len() > ri_i
                    && self.render_items[ri_i].instance_transforms.len() > ri_it_i
                {
                    self.render_items[ri_i].instance_transforms[ri_it_i].pos = (
                        trans.x / PHYSICS_DIVISOR,
                        trans.y / PHYSICS_DIVISOR,
                        trans.z / PHYSICS_DIVISOR,
                    );
                    self.render_items[ri_i].instance_transforms[ri_it_i].rot =
                        (rot[0], rot[1], rot[2], rot[3]);
                }
            }
        }
    }
}

/// Trait for operations on TextItem
pub trait TextItems {
    /// Get the len of render_items
    fn text_items_len(&self) -> usize;
    /// Get an IterMut of the TextItem
    fn text_items_iter_mut(&mut self) -> IterMut<TextItem>;
    /// Get a ref to a text item
    fn get_text_item(&mut self, index: usize) -> &mut TextItem;
    /// Get a ref to a text item from its name, returning the first found
    fn get_text_item_by_name(&mut self, name: String) -> Option<&mut TextItem>;
    /// Add a text item to the game
    fn add_text_item(&mut self, text_item: TextItem);
}

impl<T: Default> TextItems for Game<T> {
    /// Get the len of render_items
    fn text_items_len(&self) -> usize {
        self.text_items.len()
    }

    /// Get an IterMut of the TextItem
    fn text_items_iter_mut(&mut self) -> IterMut<TextItem> {
        self.text_items.par_iter_mut()
    }

    /// Get a ref to a text item
    fn get_text_item(&mut self, index: usize) -> &mut TextItem {
        &mut self.text_items[index]
    }

    /// Get a ref to a text item from its name, returning the first found
    fn get_text_item_by_name(&mut self, name: String) -> Option<&mut TextItem> {
        for i in 0..self.text_items.len() {
            if self.text_items[i].name == name {
                return Some(&mut self.text_items[i]);
            }
        }
        None
    }

    /// Add a text item to the game
    fn add_text_item(&mut self, text_item: TextItem) {
        self.text_items.push(text_item);
    }
}

/// Trait with default update definition
pub trait Update {
    /// RenderItem utype associated type
    type T;
    /// Update the per frame engine state
    fn update<F: FnMut(&Ui), U: FnMut(&mut Game<Self::T>) -> UpdateStatus>(
        &mut self,
        render_imgui: F,
        update: U,
        events: &Vec<Event<()>>,
    ) -> UpdateStatus;
    /// Update the per frame inputs
    fn update_inputs(&mut self, events: &Vec<Event<()>>);
}

/// Impl for Update on Game
impl<T: Default> Update for Game<T> {
    /// Associated type for RenderItems
    type T = T;
    /// Default Game implementation to update the engine state
    fn update<F: FnMut(&Ui), U: FnMut(&mut Game<T>) -> UpdateStatus>(
        &mut self,
        mut render_imgui: F,
        mut update: U,
        events: &Vec<Event<()>>,
    ) -> UpdateStatus {
        let frame_start = Instant::now();

        self.update_inputs(events);
        self.update_physics();

        let status = update(self);

        // render the frame
        {
            self.renderer.draw(
                &mut self.cams,
                &mut self.render_items,
                &mut self.text_items,
                &mut render_imgui,
            );
        }

        self.delta = 0.000_000_001f32 * frame_start.elapsed().subsec_nanos() as f32;

        status
    }

    /// Default Game implementation to Update inputs
    fn update_inputs(&mut self, events: &Vec<Event<()>>) {
        {
            // updating and handling the inputs
            let gl_window = self.renderer.display.gl_window();
            let window = gl_window.window();
            self.input.update_inputs(window, events);
        }
        {
            // update the inputs for imgui
            self.renderer.update_imgui_input(&self.input);
        }
    }
}

/// start running a game
pub fn start_loop<F>(event_loop: EventLoop<()>, mut callback: F) -> !
where
    F: 'static + FnMut(&Vec<Event<()>>) -> UpdateStatus,
{
    let mut events_buffer = Vec::new();
    let mut next_frame_time = Instant::now();
    event_loop.run(move |event, _, control_flow| {
        let run_callback = match event.to_static() {
            Some(Event::NewEvents(cause)) => match cause {
                StartCause::ResumeTimeReached { .. } | StartCause::Init => true,
                _ => false,
            },
            Some(event) => {
                events_buffer.push(event);
                false
            }
            None => {
                // Ignore this event.
                false
            }
        };

        let action = if run_callback {
            let action = callback(&events_buffer);
            next_frame_time = Instant::now() + Duration::from_nanos(16666667);
            // TODO: Add back the old accumulator loop in some way

            events_buffer.clear();
            action
        } else {
            UpdateStatus::Continue
        };

        match action {
            UpdateStatus::Continue => {
                *control_flow = ControlFlow::WaitUntil(next_frame_time);
            }
            UpdateStatus::Finish => *control_flow = ControlFlow::Exit,
        }
    })
}
