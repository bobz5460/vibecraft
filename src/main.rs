mod assets;
mod engine;
mod world;
mod player;
mod ui;
mod inventory;
mod gamemode;
use gamemode::{GameMode, Difficulty};

use std::collections::HashMap;
use std::sync::Arc;
use winit::event::{ElementState, Event, MouseButton, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
use winit::window::CursorGrabMode;
use rand::Rng;

use engine::camera::Camera;
use engine::input::InputState;
use engine::renderer::{ChunkRenderData, HighlightData, Renderer};
use engine::window::WindowState;
use world::block::{Block, BlockId};
use world::chunk_manager::ChunkManager;
use world::dropped_item::{map_drop, DroppedItem};
use player::Player;

fn main() {
    env_logger::init();
    pollster::block_on(run());
}

async fn run() {
    let event_loop = EventLoop::new().unwrap();
    let window_state = WindowState::new(&event_loop);

    window_state.window.set_cursor_visible(false);
    let _ = window_state.window.set_cursor_grab(CursorGrabMode::Confined);

    let window = Arc::new(window_state.window);

    let asset_path = if let Ok(path) = std::env::var("VIBECRAFT_ASSETS") {
        path
    } else {
        "/tmp/opencode/minecraft-assets".to_string()
    };
    let mut renderer = Renderer::new(window.clone(), &asset_path).await;

    let mut game_mode = GameMode::Creative;
    let mut difficulty = Difficulty::Normal;
    let mut hardcore = false;
    let mut flying = game_mode.can_fly();
    let mut player = Player::new(0.0, 100.0, 0.0);
    let mut camera = Camera::new(
        player.eye_position(),
        renderer.size.0 as f32 / renderer.size.1 as f32,
    );

    let mut input = InputState::new();
    let mut grabbed = true;

    let seed = 12345u64;
    let mut chunk_manager = ChunkManager::new(seed);

    chunk_manager.update_chunks(0, 0);
    chunk_manager.rebuild_dirty_meshes();

    let mut chunk_render_data: Vec<(i32, i32, ChunkRenderData)> = Vec::new();
    let mut render_cache: HashMap<(i32, i32), ChunkRenderData> = HashMap::new();
    rebuild_render_data(&mut chunk_render_data, &mut render_cache, &chunk_manager, &renderer);

    let mut dropped_items: Vec<DroppedItem> = Vec::new();

    let mut show_debug = false;
    let mut show_chunk_borders = false;
    let mut fps_counter = 0u32;
    let mut fps_timer = 0.0f32;
    let mut fps = 0f32;

    let block_list = vec![
        BlockId::Stone, BlockId::GrassBlock, BlockId::Dirt, BlockId::Cobblestone,
        BlockId::OakPlanks, BlockId::Sand, BlockId::Glass, BlockId::OakLog,
        BlockId::Bricks, BlockId::StoneBricks, BlockId::SnowBlock,
        BlockId::OakLeaves, BlockId::CraftingTable, BlockId::Furnace,
        BlockId::GoldBlock, BlockId::IronBlock, BlockId::DiamondBlock,
        BlockId::LapisBlock, BlockId::Bookshelf, BlockId::Gravel,
        BlockId::Netherrack, BlockId::Obsidian, BlockId::Sponge,
        BlockId::HayBlock, BlockId::Melon, BlockId::Pumpkin,
        BlockId::StoneSlab, BlockId::OakSlab,
        BlockId::StoneStairs, BlockId::OakStairs,
    ];
    let mut selected_slot: usize = 0;

    let mut break_progress: f32 = 0.0;
    let mut break_target: Option<(i32, i32, i32)> = None;

    let mut experience: u32 = 0;

    let mut game_time: f32 = 0.0;
    let mut last_time = std::time::Instant::now();
    let mut right_was_pressed = false;
    let mut highlight: Option<HighlightData> = None;
    let mut last_hit_pos: Option<(i32, i32, i32)> = None;
    let mut water_tick_counter: u32 = 0;
    let mut lava_tick_counter: u32 = 0;
    let mut command_mode = false;
    let mut command_buffer = String::new();
    let mut command_feedback = String::new();
    let mut command_feedback_timer = 0.0f32;

    let _ = event_loop.run(move |event, target| {
        target.set_control_flow(ControlFlow::Poll);

        match event {
            Event::WindowEvent { event, .. } => {
                input.handle_event(&event);

                match &event {
                    WindowEvent::CloseRequested => {
                        target.exit();
                    }
                    WindowEvent::Resized(size) => {
                        renderer.resize((size.width, size.height));
                        camera.aspect = size.width as f32 / size.height as f32;
                    }
                    WindowEvent::KeyboardInput { event: key_event, .. } => {
                        if key_event.state == ElementState::Pressed {
                            if command_mode {
                                match &key_event.logical_key {
                                    Key::Named(NamedKey::Enter) => {
                                        let cmd = command_buffer.trim().to_string();
                                        let was_gm = game_mode;
                                        execute_command(&cmd, &mut chunk_manager, &mut render_cache, &last_hit_pos, &mut command_feedback, &mut game_mode, &mut difficulty, &mut hardcore, &mut game_time, &mut dropped_items, &camera);
                                        if was_gm != game_mode {
                                            flying = game_mode.can_fly();
                                        }
                                        command_mode = false;
                                        command_buffer.clear();
                                        command_feedback_timer = 5.0;
                                    }
                                    Key::Named(NamedKey::Escape) => {
                                        command_mode = false;
                                        command_buffer.clear();
                                    }
                                    Key::Named(NamedKey::Backspace) => {
                                        command_buffer.pop();
                                    }
                                    Key::Named(NamedKey::Space) => {
                                        command_buffer.push(' ');
                                    }
                                    Key::Character(c) => {
                                        command_buffer.push_str(c.as_ref());
                                    }
                                    _ => {}
                                }
                            } else {
                                match key_event.physical_key {
                                    PhysicalKey::Code(KeyCode::Escape) => target.exit(),
                                    PhysicalKey::Code(KeyCode::F3) => show_debug = !show_debug,
                                    PhysicalKey::Code(KeyCode::KeyG) => {
                                        if input.is_key_pressed(KeyCode::F3) { show_chunk_borders = !show_chunk_borders; }
                                    }
                                    PhysicalKey::Code(KeyCode::KeyF) => {
                                        if game_mode.can_fly() { flying = !flying; player.vy = 0.0; }
                                    },
                                    PhysicalKey::Code(KeyCode::Slash) => {
                                        command_mode = true;
                                        command_buffer = "/".to_string();
                                    }
                                    PhysicalKey::Code(KeyCode::Digit1) => selected_slot = 0,
                                    PhysicalKey::Code(KeyCode::Digit2) => selected_slot = 1.min(block_list.len() - 1),
                                    PhysicalKey::Code(KeyCode::Digit3) => selected_slot = 2.min(block_list.len() - 1),
                                    PhysicalKey::Code(KeyCode::Digit4) => selected_slot = 3.min(block_list.len() - 1),
                                    PhysicalKey::Code(KeyCode::Digit5) => selected_slot = 4.min(block_list.len() - 1),
                                    PhysicalKey::Code(KeyCode::Digit6) => selected_slot = 5.min(block_list.len() - 1),
                                    PhysicalKey::Code(KeyCode::Digit7) => selected_slot = 6.min(block_list.len() - 1),
                                    PhysicalKey::Code(KeyCode::Digit8) => selected_slot = 7.min(block_list.len() - 1),
                                    PhysicalKey::Code(KeyCode::Digit9) => selected_slot = 8.min(block_list.len() - 1),
                                    _ => {}
                                }
                            }
                        }
                    }
                    WindowEvent::MouseWheel { delta, .. } => {
                        let amount = match delta {
                            winit::event::MouseScrollDelta::LineDelta(_, y) => *y as i32,
                            winit::event::MouseScrollDelta::PixelDelta(pos) => pos.y as i32,
                        };
                        if amount > 0 {
                            selected_slot = (selected_slot + 1).min(block_list.len() - 1);
                        } else if amount < 0 {
                            selected_slot = selected_slot.saturating_sub(1);
                        }
                    }
                    WindowEvent::Focused(focused) => {
                        if *focused && !grabbed {
                            grabbed = true;
                            window.set_cursor_visible(false);
                            let _ = window.set_cursor_grab(CursorGrabMode::Confined);
                        } else if !*focused && grabbed {
                            grabbed = false;
                            window.set_cursor_visible(true);
                            let _ = window.set_cursor_grab(CursorGrabMode::None);
                        }
                    }
                    _ => {}
                }
            }
            Event::DeviceEvent { event: dev_event, .. } => {
                input.handle_device_event(&dev_event);
            }
            Event::AboutToWait => {
                let now = std::time::Instant::now();
                let dt = (now - last_time).as_secs_f32().min(0.05);
                last_time = now;

                fps_counter += 1;
                fps_timer += dt;
                if fps_timer >= 1.0 {
                    fps = fps_counter as f32 / fps_timer;
                    fps_counter = 0;
                    fps_timer = 0.0;
                }

                game_time += dt; // 20 min day/night cycle (1200 game sec = 1200 real sec)

                let sprinting = input.is_key_pressed(KeyCode::ControlLeft) && !flying;
                let mut speed = 10.0 * dt;
                if input.is_key_pressed(KeyCode::ControlLeft) {
                    speed *= if flying { 5.0 } else { 1.3 };
                }

                if flying || game_mode.is_spectator() {
                    if input.is_key_pressed(KeyCode::KeyW) { camera.move_forward(speed); }
                    if input.is_key_pressed(KeyCode::KeyS) { camera.move_forward(-speed); }
                    if input.is_key_pressed(KeyCode::KeyA) { camera.move_right(speed); }
                    if input.is_key_pressed(KeyCode::KeyD) { camera.move_right(-speed); }
                    if input.is_key_pressed(KeyCode::Space) { camera.move_up(speed); }
                    if input.is_key_pressed(KeyCode::ShiftLeft) { camera.move_up(-speed); }
                    player.x = camera.position.x;
                    player.y = camera.position.y;
                    player.z = camera.position.z;
                } else if game_mode.has_gravity() {
                    let walk_speed = speed * if sprinting { 1.3 } else { 1.0 };
                    // Accumulate movement with collision
                    let mut dx = 0f32;
                    let mut dz = 0f32;
                    if input.is_key_pressed(KeyCode::KeyW) { dx += camera.yaw.sin() * walk_speed; dz += camera.yaw.cos() * walk_speed; }
                    if input.is_key_pressed(KeyCode::KeyS) { dx -= camera.yaw.sin() * walk_speed; dz -= camera.yaw.cos() * walk_speed; }
                    if input.is_key_pressed(KeyCode::KeyA) { dx += camera.yaw.cos() * walk_speed; dz += -camera.yaw.sin() * walk_speed; }
                    if input.is_key_pressed(KeyCode::KeyD) { dx -= camera.yaw.cos() * walk_speed; dz -= -camera.yaw.sin() * walk_speed; }

                    // Gravity
                    player.vy += player::GRAVITY * dt;
                    let dy = player.vy * dt;

                    player.try_move_with_difficulty(dx, dy, dz, &chunk_manager, difficulty.damage_multiplier());
                    // Natural health regen (disabled on Hard, always on Peaceful)
                    if game_mode.takes_damage() && player.health < player::MAX_HEALTH && player.health > 0.0 {
                        let regen_rate = if difficulty == Difficulty::Peaceful { 1.0 } else if difficulty.natural_regen_allowed() { 0.5 } else { 0.0 };
                        if regen_rate > 0.0 {
                            player.health = (player.health + regen_rate * dt).min(player::MAX_HEALTH);
                        }
                    }
                    // Respawn if dead (disabled for hardcore)
                    if !player.is_alive() {
                        if hardcore {
                            // Permanent death in hardcore mode: lock controls
                            flying = false;
                        } else {
                            player.health = player::MAX_HEALTH;
                            player.x = 0.0; player.y = 100.0; player.z = 0.0;
                            player.vy = 0.0;
                            flying = game_mode.can_fly();
                        }
                    }

                    if input.is_key_pressed(KeyCode::Space) && player.on_ground {
                        player.vy = player::JUMP_SPEED;
                    }

                    camera.position.x = player.x;
                    camera.position.y = player.y + player::EYE_HEIGHT;
                    camera.position.z = player.z;
                } else {
                    // Adventure mode: no gravity, no flight — static camera
                    // (just allow mouse look and raycasting)
                }

                let (dx, dy) = input.consume_mouse_delta();
                camera.rotate(dx * 0.003, dy * 0.003);

                // Raycast for block targeting and highlight
                let (origin, dir) = camera.get_ray();
                let hit = chunk_manager.raycast(origin, dir, 10.0);

                // Update highlight (only recreate when target changes)
                let hit_pos = hit.as_ref().map(|h| (h.x, h.y, h.z));
                if hit_pos != last_hit_pos {
                    highlight = hit.as_ref().map(|h| {
                        renderer.create_cube_outline(h.x as f32, h.y as f32, h.z as f32)
                    });
                    last_hit_pos = hit_pos;
                }

                // Block interaction
                let left_down = input.is_mouse_pressed(MouseButton::Left);
                let right_down = input.is_mouse_pressed(MouseButton::Right);

                // Block breaking with hold time
                let hit_pos = hit.as_ref().map(|h| (h.x, h.y, h.z));
                if left_down && hit_pos.is_some() && game_mode.can_break() {
                    if hit_pos == break_target {
                        let h = hit.as_ref().unwrap();
                        // Creative: instant break
                        if game_mode.instant_break() {
                            let drop_id = map_drop(h.block.id);
                            if drop_id != BlockId::Air && drop_id != BlockId::Water && drop_id != BlockId::Lava {
                                dropped_items.push(DroppedItem::new(
                                    h.x as f32 + 0.5, h.y as f32 + 0.5, h.z as f32 + 0.5, drop_id,
                                ));
                            }
                            chunk_manager.set_block(h.x, h.y, h.z, Block::air());
                            mark_neighbors_dirty(&mut chunk_manager, &mut render_cache, h.x, h.y, h.z);
                            break_target = None;
                            break_progress = 0.0;
                        } else {
                            // Get block hardness
                            let bt = match h.block.id {
                                BlockId::Stone | BlockId::Cobblestone | BlockId::Deepslate => 1.5,
                                BlockId::Dirt | BlockId::Sand | BlockId::Gravel | BlockId::GrassBlock => 0.5,
                                BlockId::OakPlanks | BlockId::OakLog | BlockId::Bookshelf => 2.0,
                                BlockId::Glass | BlockId::OakLeaves => 0.2,
                                BlockId::Bedrock => 999.0,
                                _ => 1.0,
                            };
                            break_progress += dt / bt;
                            if break_progress >= 1.0 {
                            let drop_id = map_drop(h.block.id);
                            if drop_id != BlockId::Air && drop_id != BlockId::Water && drop_id != BlockId::Lava {
                                dropped_items.push(DroppedItem::new(
                                    h.x as f32 + 0.5, h.y as f32 + 0.5, h.z as f32 + 0.5,
                                    drop_id,
                                ));
                            }
                            // XP from ores
                            match h.block.id {
                                BlockId::CoalOre | BlockId::DeepslateCoalOre => experience += 2,
                                BlockId::IronOre | BlockId::DeepslateIronOre => experience += 5,
                                BlockId::GoldOre | BlockId::DeepslateGoldOre => experience += 8,
                                BlockId::DiamondOre | BlockId::DeepslateDiamondOre => experience += 15,
                                BlockId::LapisOre | BlockId::DeepslateLapisOre => experience += 5,
                                BlockId::RedstoneOre | BlockId::DeepslateRedstoneOre => experience += 4,
                                BlockId::EmeraldOre | BlockId::DeepslateEmeraldOre => experience += 10,
                                _ => {}
                            }
                            // Break particles: small cubes with random velocity, short lifetime
                            for _ in 0..6 {
                                let mut p = DroppedItem::new(
                                    h.x as f32 + 0.5, h.y as f32 + 0.5, h.z as f32 + 0.5,
                                    h.block.id,
                                );
                                let angle = rand::random::<f32>() * std::f32::consts::TAU;
                                let speed = 1.0 + rand::random::<f32>() * 3.0;
                                p.vx = angle.cos() * speed;
                                p.vz = angle.sin() * speed;
                                p.vy = 3.0 + rand::random::<f32>() * 5.0;
                                p.lifetime = 0.5 + rand::random::<f32>() * 0.8;
                                dropped_items.push(p);
                            }
                            chunk_manager.set_block(h.x, h.y, h.z, Block::air());
                            mark_neighbors_dirty(&mut chunk_manager, &mut render_cache, h.x, h.y, h.z);
                            break_progress = 0.0;
                            break_target = None;
                        }
                        }
                    } else {
                        // Started targeting a new block
                        break_target = hit_pos;
                        break_progress = 0.0;
                    }
                } else {
                    break_progress = 0.0;
                    break_target = None;
                }

                if right_down && !right_was_pressed && game_mode.can_place() {
                    if let Some(ref h) = hit {
                        let px = h.x + h.normal.0;
                        let py = h.y + h.normal.1;
                        let pz = h.z + h.normal.2;
                        let existing = chunk_manager.get_block(px, py, pz);
                        if existing.is_air() {
                            let placed = block_list[selected_slot];
                            chunk_manager.set_block(px, py, pz, Block::new(placed));
                            mark_neighbors_dirty(&mut chunk_manager, &mut render_cache, px, py, pz);
                        }
                    }
                }
                right_was_pressed = right_down;

                // Update dropped items
                dropped_items.retain(|item| item.is_alive());
                for item in &mut dropped_items {
                    item.update(dt);
                }
                // Item pickup: remove items within 2 blocks of the player
                let px = if flying { camera.position.x } else { player.x };
                let py = if flying { camera.position.y } else { player.y + player::EYE_HEIGHT * 0.5 };
                let pz = if flying { camera.position.z } else { player.z };
                dropped_items.retain(|item| {
                    let dx = item.x - px;
                    let dy = item.y - py;
                    let dz = item.z - pz;
                    let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                    if dist < 2.0 && item.lifetime < 290.0 {
                        experience += 1;
                        false // remove
                    } else {
                        true
                    }
                });

                let pcx = (camera.position.x / 16.0).round() as i32;
                let pcz = (camera.position.z / 16.0).round() as i32;
                chunk_manager.update_chunks_async(pcx, pcz);
                chunk_manager.process_loaded_chunks();
                chunk_manager.rebuild_dirty_meshes();
                water_tick_counter += 1;
                if water_tick_counter >= 5 {
                    water_tick_counter = 0;
                    chunk_manager.tick_water();
                }
                lava_tick_counter += 1;
                if lava_tick_counter >= 8 {
                    lava_tick_counter = 0;
                    chunk_manager.tick_lava();
                }
    rebuild_render_data(&mut chunk_render_data, &mut render_cache, &chunk_manager, &renderer);

                // Render dropped items
                if !dropped_items.is_empty() {
                    let item_mesh = world::dropped_item::dropped_items_to_mesh(&dropped_items);
                    if !item_mesh.vertices.is_empty() {
                        let item_data = renderer.create_chunk_data(&item_mesh);
                        chunk_render_data.push((i32::MAX, i32::MAX, item_data));
                    }
                }

                let feedback_visible = command_feedback_timer > 0.0 && !command_feedback.is_empty();
                let debug_lines: Vec<String> = if show_debug || feedback_visible {
                    let pos = format!("XYZ: {:.1} / {:.1} / {:.1}", camera.position.x, camera.position.y, camera.position.z);
                    let block = match &hit {
                        Some(h) => format!("Looking at: {} ({},{},{})", h.block.id.name(), h.x, h.y, h.z),
                        None => "Looking at: nothing".to_string(),
                    };
                    let biome = format!("Biome: {}", chunk_manager.get_biome_name(camera.position.x as f64, camera.position.z as f64));
                    let facing = format!("Facing: {:.1}° / {:.1}°", camera.yaw.to_degrees(), camera.pitch.to_degrees());
                    let break_info = if break_target.is_some() {
                        format!("Break: {:.0}%", break_progress * 100.0)
                    } else {
                        String::new()
                    };
                    let mut lines = vec![
                        format!("Vibecraft  FPS: {:.0}", fps),
                        pos,
                        block,
                        biome,
                        facing,
                        format!("Time: {:.0}s  HP: {:.0}/{}  Mode: {}  Diff: {}{}  Flying: {}  Borders: {}", game_time, player.health, player::MAX_HEALTH, game_mode.name(), difficulty.name(), if hardcore { " (HC)" } else { "" }, if flying { "yes" } else { "no" }, if show_chunk_borders { "ON" } else { "OFF" }),
                    ];
                    if !break_info.is_empty() {
                        lines.push(break_info);
                    }
                    if feedback_visible {
                        lines.push(command_feedback.clone());
                    }
                    lines
                } else if feedback_visible {
                    vec![command_feedback.clone()]
                } else {
                    Vec::new()
                };

                // Command feedback timer
                command_feedback_timer = (command_feedback_timer - dt).max(0.0);

                // Build hotbar text (always visible)
                let hotbar = if command_mode {
                    format!("{}_", command_buffer)
                } else {
                    let slots_per_row = 9;
                    let mut h = String::new();
                    for i in 0..block_list.len().min(slots_per_row) {
                        if i > 0 { h.push(' '); }
                        if i == selected_slot % slots_per_row {
                            h.push('>');
                        }
                        h.push_str(block_list[i].name());
                        if i == selected_slot % slots_per_row {
                            h.push('<');
                        }
                    }
                    h
                };

                // Chunk border debug lines
                let border_data = if show_chunk_borders {
                    let mut verts: Vec<[f32; 3]> = Vec::new();
                    for (&(cx, cz), _chunk) in &chunk_manager.chunks {
                        let x0 = (cx * 16) as f32;
                        let z0 = (cz * 16) as f32;
                        let x1 = x0 + 16.0;
                        let z1 = z0 + 16.0;
                        // 4 vertical corner lines
                        for &(x, z) in &[(x0, z0), (x1, z0), (x1, z1), (x0, z1)] {
                            verts.push([x, 0.0, z]);
                            verts.push([x, 384.0, z]);
                        }
                        // 4 bottom edges
                        verts.push([x0, 0.0, z0]); verts.push([x1, 0.0, z0]);
                        verts.push([x1, 0.0, z0]); verts.push([x1, 0.0, z1]);
                        verts.push([x1, 0.0, z1]); verts.push([x0, 0.0, z1]);
                        verts.push([x0, 0.0, z1]); verts.push([x0, 0.0, z0]);
                        // 4 top edges
                        verts.push([x0, 384.0, z0]); verts.push([x1, 384.0, z0]);
                        verts.push([x1, 384.0, z0]); verts.push([x1, 384.0, z1]);
                        verts.push([x1, 384.0, z1]); verts.push([x0, 384.0, z1]);
                        verts.push([x0, 384.0, z1]); verts.push([x0, 384.0, z0]);
                    }
                    let vb = renderer.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("border_vb"),
                        size: (verts.len() as u64) * std::mem::size_of::<[f32; 3]>() as u64,
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    renderer.queue.write_buffer(&vb, 0, bytemuck::cast_slice(&verts));
                    Some((vb, verts.len() as u32))
                } else {
                    None
                };

                match renderer.render(&camera, &chunk_render_data, highlight.as_ref(),
                    border_data.as_ref(),
                    if show_debug { Some(&debug_lines) } else { None },
                    &hotbar,
                ) {
                    Ok(_) => {}
                    Err(wgpu::SurfaceError::Lost) => renderer.resize(renderer.size),
                    Err(wgpu::SurfaceError::OutOfMemory) => target.exit(),
                    Err(_) => {}
                }
            }
            _ => {}
        }
    });
}

fn execute_command(
    cmd: &str,
    cm: &mut ChunkManager,
    cache: &mut HashMap<(i32, i32), ChunkRenderData>,
    target: &Option<(i32, i32, i32)>,
    feedback: &mut String,
    game_mode: &mut GameMode,
    difficulty: &mut Difficulty,
    hardcore: &mut bool,
    game_time: &mut f32,
    dropped_items: &mut Vec<DroppedItem>,
    camera: &Camera,
) {
    // Extract the command parts:
    //   /<action> [<subj> [<args>...]]
    let parts: Vec<&str> = cmd.trim_start_matches('/').split_whitespace().collect();
    let action = parts.first().copied().unwrap_or("");
    let subj = parts.get(1).copied().unwrap_or("");
    let rest: Vec<&str> = parts[2..].to_vec();

    // Handle game mode commands (no target needed)
    if action == "gamemode" || action == "gm" {
        if let Some(mode) = GameMode::from_str(subj) {
            if *hardcore && mode != GameMode::Survival {
                *feedback = "Cannot change game mode in hardcore mode!".to_string();
                return;
            }
            *game_mode = mode;
            *feedback = format!("Set game mode to {}", mode.name());
        } else if subj.is_empty() {
            *feedback = format!("Current game mode: {}", game_mode.name());
        } else {
            *feedback = format!("Unknown game mode: {}. Use: survival, creative, adventure, spectator", subj);
        }
        return;
    }

    // Handle difficulty command
    if action == "difficulty" || action == "d" {
        if *hardcore {
            *feedback = "Cannot change difficulty in hardcore mode (locked to Hard).".to_string();
            return;
        }
        if let Some(d) = Difficulty::from_str(subj) {
            *difficulty = d;
            *feedback = format!("Set difficulty to {}", d.name());
        } else if subj.is_empty() {
            *feedback = format!("Current difficulty: {}", difficulty.name());
        } else {
            *feedback = format!("Unknown difficulty: {}. Use: peaceful, easy, normal, hard", subj);
        }
        return;
    }

    // Handle hardcore command
    if action == "hardcore" || action == "hc" {
        *hardcore = true;
        *difficulty = Difficulty::Hard;
        *game_mode = GameMode::Survival;
        *feedback = "Hardcore mode enabled! Difficulty locked to Hard, permanent death.".to_string();
        return;
    }

    // Handle time set command
    if action == "time" && subj == "set" {
        let arg = rest.first().copied().unwrap_or("");
        let new_time = match arg.to_lowercase().as_str() {
            "day" => Some(300.0),
            "noon" => Some(300.0),
            "night" => Some(900.0),
            "midnight" => Some(900.0),
            _ => arg.parse::<f32>().ok(),
        };
        if let Some(t) = new_time {
            *game_time = t;
            *feedback = format!("Set time to {:.0}s ({})", t, arg);
        } else {
            *feedback = format!("Usage: /time set <0-1200|day|night>");
        }
        return;
    }

    // Handle give command (no target needed)
    if action == "give" {
        let block_name = subj;
        let count = rest.first().and_then(|s| s.parse::<usize>().ok()).unwrap_or(1).min(64);
        if let Some(id) = BlockId::from_name(block_name) {
            let px = camera.position.x;
            let py = camera.position.y;
            let pz = camera.position.z;
            for _ in 0..count {
                let x_off = rand::random::<f32>() * 0.5 - 0.25;
                let z_off = rand::random::<f32>() * 0.5 - 0.25;
                let mut item = DroppedItem::new(px + x_off, py, pz + z_off, map_drop(id));
                item.vy = 3.0 + rand::random::<f32>() * 2.0;
                dropped_items.push(item);
            }
            *feedback = format!("Gave {} x {}", count, id.name());
        } else {
            *feedback = format!("Unknown block: {}. Try /give stone, /give dirt, etc.", block_name);
        }
        return;
    }

    // Structure commands require a target
    let pos = match target {
        Some(p) => *p,
        None => {
            *feedback = "No block targeted. Look at a block first.".to_string();
            return;
        }
    };

    let mut rng = rand::thread_rng();

    // Determine what structure to spawn
    let structure = match (action, subj) {
        ("summon", name) | ("place", name) => name,
        _ => action,
    };

    match structure {
        "dungeon" | "d" => {
            spawn_dungeon(cm, pos.0, pos.1, pos.2, &mut rng, cache);
            *feedback = format!("Summoned dungeon at ({}, {}, {})", pos.0, pos.1, pos.2);
        }
        "portal" | "ruined_portal" | "p" => {
            spawn_ruined_portal(cm, pos.0, pos.1, pos.2, &mut rng, cache);
            *feedback = format!("Summoned ruined portal at ({}, {}, {})", pos.0, pos.1, pos.2);
        }
        "lava" | "lava_pool" | "l" => {
            spawn_lava_pool(cm, pos.0, pos.1, pos.2, &mut rng, cache);
            *feedback = format!("Summoned lava pool at ({}, {}, {})", pos.0, pos.1, pos.2);
        }
        "mushroom" | "giant_mushroom" | "m" => {
            spawn_giant_mushroom(cm, pos.0, pos.1, pos.2, rng.gen_bool(0.5), &mut rng, cache);
            *feedback = format!("Summoned giant mushroom at ({}, {}, {})", pos.0, pos.1, pos.2);
        }
        "tree" | "oak" | "t" => {
            spawn_tree(cm, pos.0, pos.1, pos.2, &mut rng, cache);
            *feedback = format!("Summoned oak tree at ({}, {}, {})", pos.0, pos.1, pos.2);
        }
        "igloo" | "i" => {
            spawn_igloo_command(cm, pos.0, pos.1, pos.2, &mut rng, cache);
            *feedback = format!("Summoned igloo at ({}, {}, {})", pos.0, pos.1, pos.2);
        }
        "swamp_hut" | "hut" | "sh" => {
            spawn_swamp_hut_command(cm, pos.0, pos.1, pos.2, &mut rng, cache);
            *feedback = format!("Summoned swamp hut at ({}, {}, {})", pos.0, pos.1, pos.2);
        }
        "well" | "desert_well" | "w" => {
            spawn_desert_well_command(cm, pos.0, pos.1, pos.2, &mut rng, cache);
            *feedback = format!("Summoned desert well at ({}, {}, {})", pos.0, pos.1, pos.2);
        }
        "ruin" | "ocean_ruin" | "r" => {
            spawn_ocean_ruin_command(cm, pos.0, pos.1, pos.2, &mut rng, cache);
            *feedback = format!("Summoned ocean ruin at ({}, {}, {})", pos.0, pos.1, pos.2);
        }
        "help" | "?" | "h" => {
            *feedback = "Commands: /summon <struct>, /gamemode <mode>, /difficulty <level>, /hardcore, /time set <val>, /give <block> [n]".to_string();
        }
        _ => {
            *feedback = format!("Unknown: /{}. Try /help", structure);
        }
    }
}

fn mark_block(_cache: &mut HashMap<(i32, i32), ChunkRenderData>, cm: &mut ChunkManager, x: i32, y: i32, z: i32, id: BlockId) {
    if y <= 0 || y >= 384 { return; }
    cm.set_block(x, y, z, Block::new(id));
}

fn mark_area_dirty(cache: &mut HashMap<(i32, i32), ChunkRenderData>, x: i32, _y: i32, z: i32, rx: i32, _ry: i32, rz: i32) {
    for dx in -rx..=rx {
        for dz in -rz..=rz {
            let cx = (x + dx).div_euclid(16);
            let cz = (z + dz).div_euclid(16);
            cache.remove(&(cx, cz));
        }
    }
}

fn spawn_dungeon(cm: &mut ChunkManager, x: i32, y: i32, z: i32, rng: &mut impl rand::Rng, cache: &mut HashMap<(i32, i32), ChunkRenderData>) {
    let hw = 3i32; let hh = 2i32; let hd = 3i32;
    for dx in -hw-1..=hw+1 {
        for dz in -hd-1..=hd+1 {
            for dy in -hh-1..=hh+2 {
                let bx = x + dx; let by = y + dy; let bz = z + dz;
                let is_wall = dx == -hw-1 || dx == hw+1 || dz == -hd-1 || dz == hd+1 || dy == -hh-1 || dy == hh+2;
                if is_wall {
                    let id = if rng.gen_bool(0.3) { BlockId::MossyCobblestone } else { BlockId::Cobblestone };
                    mark_block(cache, cm, bx, by, bz, id);
                } else {
                    mark_block(cache, cm, bx, by, bz, BlockId::Air);
                }
            }
        }
    }
    // Spawner in center
    mark_block(cache, cm, x, y, z, BlockId::Spawner);
    // Chests
    for _ in 0..1 + (rng.gen::<f64>() * 1.5) as usize {
        let (cx, cz) = match rng.gen_range(0..4) {
            0 => (x - hw + 1 + rng.gen_range(0..hw*2 - 1), z - hd),
            1 => (x - hw + 1 + rng.gen_range(0..hw*2 - 1), z + hd),
            2 => (x - hw, z - hd + 1 + rng.gen_range(0..hd*2 - 1)),
            _ => (x + hw, z - hd + 1 + rng.gen_range(0..hd*2 - 1)),
        };
        if cm.get_block(cx, y, cz).is_air() {
            mark_block(cache, cm, cx, y, cz, BlockId::Chest);
        }
    }
    mark_area_dirty(cache, x, y, z, hw+2, hh+3, hd+2);
}

fn spawn_ruined_portal(cm: &mut ChunkManager, x: i32, y: i32, z: i32, rng: &mut impl rand::Rng, cache: &mut HashMap<(i32, i32), ChunkRenderData>) {
    let fw = 4i32; let fh = 5i32;
    let mut missing = 2 + (rng.gen::<f64>() * 4.0) as i32;
    for dy in 0i32..fh {
        for dx in 0i32..fw {
            let bx = x - fw/2 + dx;
            let by = y + dy;
            let bz = z;
            let is_frame = dx == 0 || dx == fw - 1 || dy == 0 || dy == fh - 1;
            let is_top_corner = (dx == 0 || dx == fw - 1) && dy == fh - 1;
            if !is_frame || is_top_corner { continue; }
            if missing > 0 && !(dx == 0 || dx == fw - 1) && dy > 0 {
                missing -= 1; continue;
            }
            let id = if dy == 0 && rng.gen_bool(0.4) { BlockId::StoneBricks }
                      else if rng.gen_bool(0.1) { BlockId::CryingObsidian }
                      else { BlockId::Obsidian };
            mark_block(cache, cm, bx, by, bz, id);
        }
    }
    // Stone bricks around base
    for dx in -2i32..=fw+1 {
        for dz in -2i32..=2 {
            let bx = x - fw/2 + dx;
            let bz = z + dz;
            if dx < 0 || dx >= fw || dz < -1 || dz > 1 {
                if rng.gen_bool(0.3) {
                    mark_block(cache, cm, bx, y, bz, BlockId::StoneBricks);
                }
            }
        }
    }
    // Vines
    for _ in 0..3 {
        let vx = x - fw/2 + rng.gen_range(0..fw);
        if rng.gen_bool(0.5) {
            let side = if rng.gen_bool(0.5) { -1 } else { 1 };
            let target = cm.get_block(vx + side, y + 2, z);
            if target.is_air() {
                mark_block(cache, cm, vx + side, y + 2, z, BlockId::Vine);
            }
        }
    }
    mark_area_dirty(cache, x, y, z, fw+3, fh+2, 3);
}

fn spawn_lava_pool(cm: &mut ChunkManager, x: i32, y: i32, z: i32, rng: &mut impl rand::Rng, cache: &mut HashMap<(i32, i32), ChunkRenderData>) {
    let radius_i = 1 + (rng.gen::<f64>() * 1.5) as i32;
    for dx in -radius_i..=radius_i {
        for dz in -radius_i..=radius_i {
            let bx = x + dx; let bz = z + dz;
            let dist = (dx as f64).powi(2) + (dz as f64).powi(2);
            if dist > (radius_i as f64 + 0.5).powi(2) { continue; }
            mark_block(cache, cm, bx, y, bz, BlockId::Stone);
            let is_lava = dx.abs() <= radius_i - 1 && dz.abs() <= radius_i - 1;
            if is_lava {
                mark_block(cache, cm, bx, y + 1, bz, BlockId::Lava);
            }
        }
    }
    mark_area_dirty(cache, x, y, z, radius_i + 1, 2, radius_i + 1);
}

fn spawn_giant_mushroom(cm: &mut ChunkManager, x: i32, y: i32, z: i32, is_red: bool, rng: &mut impl rand::Rng, cache: &mut HashMap<(i32, i32), ChunkRenderData>) {
    let stem_h = 2i32 + (rng.gen::<f64>() * 2.0) as i32;
    let cap_id = if is_red { BlockId::RedMushroomBlock } else { BlockId::BrownMushroomBlock };
    for dy in 1i32..=stem_h {
        mark_block(cache, cm, x, y + dy, z, BlockId::MushroomStem);
    }
    let cy = y + stem_h + 1;
    for dx in -2i32..=2i32 {
        for dz in -2i32..=2i32 {
            for dy in 0i32..=2i32 {
                let bx = x + dx; let bz = z + dz; let by = cy + dy;
                let d = dx.abs().max(dz.abs());
                let in_range = match dy {
                    0i32 => d <= 2i32 && d > 0i32,
                    1i32 => d <= 2i32,
                    2i32 => d <= 1i32,
                    _ => false,
                };
                if in_range {
                    mark_block(cache, cm, bx, by, bz, cap_id);
                }
            }
        }
    }
    mark_block(cache, cm, x, cy + 2i32, z, cap_id);
    mark_area_dirty(cache, x, y, z, 3i32, stem_h + 4i32, 3i32);
}

fn spawn_tree(cm: &mut ChunkManager, x: i32, y: i32, z: i32, rng: &mut impl rand::Rng, cache: &mut HashMap<(i32, i32), ChunkRenderData>) {
    let trunk_h: i32 = 5 + (rng.gen::<f64>().abs() * 2.0) as i32;
    for dy in 1..=trunk_h.saturating_sub(1) {
        mark_block(cache, cm, x, y + dy, z, BlockId::OakLog);
    }
    let leaf_start = y + trunk_h - 2;
    for dy in 0..=2i32 {
        let radius: i32 = if dy == 0 { 2 } else if dy == 1 { 2 } else { 1 };
        for dx in -radius..=radius {
            for dz in -radius..=radius {
                if dx.abs() == radius && dz.abs() == radius && dy > 0 { continue; }
                let bx = x + dx; let bz = z + dz; let by = leaf_start + dy;
                if cm.get_block(bx, by, bz).is_air() {
                    mark_block(cache, cm, bx, by, bz, BlockId::OakLeaves);
                }
            }
        }
    }
    mark_area_dirty(cache, x, y, z, 3, trunk_h + 2, 3);
}

fn spawn_igloo_command(cm: &mut ChunkManager, x: i32, y: i32, z: i32, _rng: &mut impl rand::Rng, cache: &mut HashMap<(i32, i32), ChunkRenderData>) {
    let gy = y.max(1);
    // Snow floor
    let floor_y = gy;
    for dy in 0..3 {
        let r = match dy { 0 => 2i32, 1 => 1i32, _ => 0i32 };
        for dx in -(r as i32)..=r as i32 {
            for dz in -(r as i32)..=r as i32 {
                let bx = x + dx; let bz = z + dz; let by = floor_y + dy;
                if (dx.abs() == r as i32 || dz.abs() == r as i32 || dy == 2) && r > 0 {
                    mark_block(cache, cm, bx, by, bz, BlockId::SnowBlock);
                } else if dy == 0 {
                    mark_block(cache, cm, bx, by, bz, if dx == 0 && dz == 0 { BlockId::RedCarpet } else { BlockId::WhiteCarpet });
                } else {
                    mark_block(cache, cm, bx, by, bz, BlockId::Air);
                }
            }
        }
    }
    mark_block(cache, cm, x, floor_y + 1, z, BlockId::Furnace);
    if x + 1 < 1_000_000 {
        mark_block(cache, cm, x + 1, floor_y, z, BlockId::RedWool);
    }
    mark_area_dirty(cache, x, gy, z, 3, 4, 3);
}

fn spawn_swamp_hut_command(cm: &mut ChunkManager, x: i32, y: i32, z: i32, _rng: &mut impl rand::Rng, cache: &mut HashMap<(i32, i32), ChunkRenderData>) {
    let gy = y;
    let floor_y = gy;
    // 3x3 oak plank floor
    for dx in 0..3 {
        for dz in 0..3 {
            mark_block(cache, cm, x + dx, floor_y, z + dz, BlockId::OakPlanks);
        }
    }
    // Walls 2 high
    for dy in 1..=2 {
        for dx in 0..3 { for dz in 0..3 {
            let is_wall = dx == 0 || dx == 2 || dz == 0 || dz == 2;
            let is_door = dx == 1 && dz == 0 && dy == 1;
            if is_wall && !is_door {
                mark_block(cache, cm, x + dx, floor_y + dy, z + dz, BlockId::OakPlanks);
            }
        }}
    }
    // Roof
    for dx in -1i32..=3 {
        for dz in -1i32..=3 {
            let d = dx.abs().max(dz.abs());
            if d <= 2 {
                mark_block(cache, cm, x + dx, floor_y + 3, z + dz, BlockId::OakPlanks);
            }
        }
    }
    // Mushroom inside
    mark_block(cache, cm, x + 1, floor_y + 1, z + 1, BlockId::BrownMushroom);
    mark_area_dirty(cache, x, gy, z, 3, 4, 3);
}

fn spawn_desert_well_command(cm: &mut ChunkManager, x: i32, y: i32, z: i32, _rng: &mut impl rand::Rng, cache: &mut HashMap<(i32, i32), ChunkRenderData>) {
    let gy = y;
    for dx in 0..2 { for dz in 0..2 {
        mark_block(cache, cm, x + dx, gy + 1, z + dz, BlockId::Water);
    }}
    for (rx, rz) in &[(-1i32,-1i32),(-1,0),(-1,1),(-1,2),(2,-1),(2,0),(2,1),(2,2),
                       (0,-1),(1,-1),(0,2),(1,2)] {
        mark_block(cache, cm, x + rx, gy + 1, z + rz, BlockId::StoneBricks);
    }
    mark_area_dirty(cache, x, gy, z, 3, 2, 3);
}

fn spawn_ocean_ruin_command(cm: &mut ChunkManager, x: i32, y: i32, z: i32, rng: &mut impl rand::Rng, cache: &mut HashMap<(i32, i32), ChunkRenderData>) {
    let gy = y;
    for dx in -1i32..=1 { for dz in -1i32..=1 {
        mark_block(cache, cm, x + dx, gy, z + dz, BlockId::StoneBricks);
        if dx.abs() == 1 && dz.abs() == 1 && rng.gen_bool(0.6) {
            mark_block(cache, cm, x + dx, gy + 1, z + dz, BlockId::StoneBricks);
        }
    }}
    mark_area_dirty(cache, x, gy, z, 2, 2, 2);
}

fn mark_neighbors_dirty(cm: &mut ChunkManager, cache: &mut HashMap<(i32, i32), ChunkRenderData>, x: i32, _y: i32, z: i32) {
    for (dx, dz) in &[(0, 0), (-1, 0), (1, 0), (0, -1), (0, 1)] {
        let cx = (x + dx).div_euclid(16);
        let cz = (z + dz).div_euclid(16);
        if let Some(c) = cm.chunks.get_mut(&(cx, cz)) {
            c.is_dirty = true;
        }
        cache.remove(&(cx, cz));
    }
}

fn rebuild_render_data(
    data: &mut Vec<(i32, i32, ChunkRenderData)>,
    cache: &mut HashMap<(i32, i32), ChunkRenderData>,
    manager: &ChunkManager,
    renderer: &Renderer,
) {
    cache.retain(|key, _| manager.chunks.contains_key(key));

    for (&(cx, cz), chunk) in &manager.chunks {
        if chunk.has_mesh && !cache.contains_key(&(cx, cz)) {
            if let Some(mesh) = manager.get_chunk_mesh(cx, cz) {
                if mesh.vertices.is_empty() && mesh.transparent_vertices.is_empty() {
                    continue;
                }
                let rd = renderer.create_chunk_data(mesh);
                cache.insert((cx, cz), rd);
            }
        }
    }

    data.clear();
    for (&(cx, cz), rd) in cache.iter() {
        data.push((cx, cz, rd.clone()));
    }
}
