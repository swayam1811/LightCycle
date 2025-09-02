use ggez::{
    event::{self, EventHandler},
    graphics::{self, Color, DrawMode, DrawParam, MeshBuilder, Rect},
    input::keyboard::{KeyCode, KeyInput},
    mint::Point2,
    Context, ContextBuilder, GameResult,
};
use rand::Rng;
use std::collections::VecDeque;

const GRID_WIDTH: f32 = 1600.0;
const GRID_HEIGHT: f32 = 1000.0;
const CELL_SIZE: f32 = 8.0;
const CYCLE_SPEED: f32 = 3.0;
const BOOST_SPEED: f32 = 6.0;
const TRAIL_MAX_LENGTH: usize = 15000;
const CYCLE_WIDTH: f32 = 16.0;
const CYCLE_HEIGHT: f32 = 24.0;
const MAX_BOOST_ENERGY: f32 = 100.0;
const BOOST_DRAIN_RATE: f32 = 40.0; // Energy per second
const BOOST_RECHARGE_RATE: f32 = 15.0; // Energy per second

#[derive(Clone, Copy, Debug, PartialEq)]
enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl Direction {
    fn to_velocity(&self) -> (f32, f32) {
        match self {
            Direction::Up => (0.0, -CYCLE_SPEED),
            Direction::Down => (0.0, CYCLE_SPEED),
            Direction::Left => (-CYCLE_SPEED, 0.0),
            Direction::Right => (CYCLE_SPEED, 0.0),
        }
    }

    fn is_opposite(&self, other: &Direction) -> bool {
        matches!(
            (self, other),
            (Direction::Up, Direction::Down)
                | (Direction::Down, Direction::Up)
                | (Direction::Left, Direction::Right)
                | (Direction::Right, Direction::Left)
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PlayerType {
    Human,
    Computer,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum AIDifficulty {
    Easy,
    Medium,
    Hard,
}

struct Explosion {
    _position: Point2<f32>,
    particles: Vec<Particle>,
    time: f32,
}

struct Particle {
    position: Point2<f32>,
    velocity: Point2<f32>,
    lifetime: f32,
    color: Color,
}

impl Explosion {
    fn new(position: Point2<f32>, color: Color) -> Self {
        let mut rng = rand::thread_rng();
        let particles = (0..50)
            .map(|_| {
                let angle = rng.gen_range(0.0..std::f32::consts::TAU);
                let speed = rng.gen_range(50.0..200.0);
                Particle {
                    position,
                    velocity: Point2 {
                        x: angle.cos() * speed,
                        y: angle.sin() * speed,
                    },
                    lifetime: rng.gen_range(0.5..1.5),
                    color: Color::new(
                        color.r,
                        color.g,
                        color.b,
                        rng.gen_range(0.5..1.0),
                    ),
                }
            })
            .collect();

        Explosion {
            _position: position,
            particles,
            time: 0.0,
        }
    }

    fn update(&mut self, dt: f32) {
        self.time += dt;
        for particle in &mut self.particles {
            particle.position.x += particle.velocity.x * dt;
            particle.position.y += particle.velocity.y * dt;
            particle.lifetime -= dt;
            particle.velocity.x *= 0.98;
            particle.velocity.y *= 0.98;
        }
        self.particles.retain(|p| p.lifetime > 0.0);
    }

    fn is_finished(&self) -> bool {
        self.particles.is_empty()
    }
}

struct LightCycle {
    position: Point2<f32>,
    direction: Direction,
    trail: VecDeque<Point2<f32>>,
    color: Color,
    alive: bool,
    player_type: PlayerType,
    controls: Option<(KeyCode, KeyCode, KeyCode, KeyCode)>, // up, down, left, right
    boost_energy: f32,
    is_boosting: bool,
    boost_key: Option<KeyCode>,
    ai_difficulty: AIDifficulty,
}

impl LightCycle {
    fn new(
        x: f32,
        y: f32,
        direction: Direction,
        color: Color,
        player_type: PlayerType,
        controls: Option<(KeyCode, KeyCode, KeyCode, KeyCode)>,
        boost_key: Option<KeyCode>,
        ai_difficulty: AIDifficulty,
    ) -> Self {
        LightCycle {
            position: Point2 { x, y },
            direction,
            trail: VecDeque::new(),
            color,
            alive: true,
            player_type,
            controls,
            boost_energy: MAX_BOOST_ENERGY,
            is_boosting: false,
            boost_key,
            ai_difficulty,
        }
    }

    fn update(&mut self, dt: f32, all_trails: &[VecDeque<Point2<f32>>], own_index: usize) {
        if !self.alive {
            return;
        }

        // Handle boost energy
        if self.is_boosting && self.boost_energy > 0.0 {
            self.boost_energy = (self.boost_energy - BOOST_DRAIN_RATE * dt).max(0.0);
            if self.boost_energy == 0.0 {
                self.is_boosting = false;
            }
        } else if !self.is_boosting && self.boost_energy < MAX_BOOST_ENERGY {
            self.boost_energy = (self.boost_energy + BOOST_RECHARGE_RATE * dt).min(MAX_BOOST_ENERGY);
        }

        let speed = if self.is_boosting { BOOST_SPEED } else { CYCLE_SPEED };
        let velocity = match self.direction {
            Direction::Up => (0.0, -speed),
            Direction::Down => (0.0, speed),
            Direction::Left => (-speed, 0.0),
            Direction::Right => (speed, 0.0),
        };
        let old_pos = self.position;
        
        self.position.x += velocity.0;
        self.position.y += velocity.1;

        // Add intermediate points for smoother trail
        let distance = ((self.position.x - old_pos.x).powi(2) + 
                       (self.position.y - old_pos.y).powi(2)).sqrt();
        let steps = (distance / 2.0).ceil() as usize;
        
        for i in 0..=steps {
            let t = i as f32 / steps.max(1) as f32;
            let interpolated = Point2 {
                x: old_pos.x + (self.position.x - old_pos.x) * t,
                y: old_pos.y + (self.position.y - old_pos.y) * t,
            };
            self.trail.push_back(interpolated);
        }

        if self.trail.len() > TRAIL_MAX_LENGTH {
            self.trail.pop_front();
        }

        // Check wall collision
        if self.position.x < 0.0
            || self.position.x >= GRID_WIDTH
            || self.position.y < 0.0
            || self.position.y >= GRID_HEIGHT
        {
            self.alive = false;
            return;
        }

        // Check trail collision
        for (i, trail) in all_trails.iter().enumerate() {
            let check_range = if i == own_index {
                // For own trail, skip recent points to avoid self-collision on turns
                trail.len().saturating_sub(10)
            } else {
                trail.len()
            };

            for point in trail.iter().take(check_range) {
                let dist = ((self.position.x - point.x).powi(2) + 
                           (self.position.y - point.y).powi(2)).sqrt();
                if dist < CELL_SIZE {
                    self.alive = false;
                    return;
                }
            }
        }
    }

    fn ai_update(&mut self, all_trails: &[VecDeque<Point2<f32>>], _own_index: usize) {
        if self.player_type != PlayerType::Computer || !self.alive {
            return;
        }

        let mut rng = rand::thread_rng();
        
        // Adjust AI parameters based on difficulty
        let (look_ahead, reaction_distance, turn_chance, boost_threshold, boost_chance) = match self.ai_difficulty {
            AIDifficulty::Easy => (20.0, CELL_SIZE * 3.0, 5, 30.0, 1),
            AIDifficulty::Medium => (40.0, CELL_SIZE * 5.0, 2, 50.0, 3),
            AIDifficulty::Hard => (60.0, CELL_SIZE * 8.0, 1, 70.0, 5),
        };

        // Check if we need to turn
        let current_velocity = self.direction.to_velocity();
        let future_x = self.position.x + current_velocity.0 * look_ahead;
        let future_y = self.position.y + current_velocity.1 * look_ahead;

        let mut should_turn = false;
        
        // Check for wall collision
        if future_x < 10.0 || future_x >= GRID_WIDTH - 10.0 
            || future_y < 10.0 || future_y >= GRID_HEIGHT - 10.0 {
            should_turn = true;
        }

        // Check for trail collision
        if !should_turn {
            for (_i, trail) in all_trails.iter().enumerate() {
                for point in trail.iter() {
                    let dist_to_future = ((future_x - point.x).powi(2) + 
                                         (future_y - point.y).powi(2)).sqrt();
                    if dist_to_future < reaction_distance {
                        should_turn = true;
                        break;
                    }
                }
                if should_turn {
                    break;
                }
            }
        }

        if should_turn {
            let possible_dirs = [Direction::Up, Direction::Down, Direction::Left, Direction::Right];
            let mut safe_dirs = Vec::new();

            for dir in &possible_dirs {
                if dir.is_opposite(&self.direction) {
                    continue;
                }

                let test_velocity = dir.to_velocity();
                let test_x = self.position.x + test_velocity.0 * look_ahead;
                let test_y = self.position.y + test_velocity.1 * look_ahead;

                // Check if this direction is safe
                let mut is_safe = test_x >= 10.0 && test_x < GRID_WIDTH - 10.0 
                    && test_y >= 10.0 && test_y < GRID_HEIGHT - 10.0;

                if is_safe {
                    for trail in all_trails.iter() {
                        for point in trail.iter() {
                            let dist = ((test_x - point.x).powi(2) + 
                                       (test_y - point.y).powi(2)).sqrt();
                            if dist < reaction_distance {
                                is_safe = false;
                                break;
                            }
                        }
                        if !is_safe {
                            break;
                        }
                    }
                }

                if is_safe {
                    safe_dirs.push(*dir);
                }
            }

            if !safe_dirs.is_empty() {
                // Hard AI chooses more optimal paths
                if self.ai_difficulty == AIDifficulty::Hard && safe_dirs.len() > 1 {
                    // Choose direction with most open space
                    let mut best_dir = safe_dirs[0];
                    let mut max_space = 0.0;
                    
                    for dir in &safe_dirs {
                        let test_velocity = dir.to_velocity();
                        let mut space = 0.0;
                        for i in 1..10 {
                            let check_x = self.position.x + test_velocity.0 * (i as f32 * 10.0);
                            let check_y = self.position.y + test_velocity.1 * (i as f32 * 10.0);
                            if check_x >= 0.0 && check_x < GRID_WIDTH && check_y >= 0.0 && check_y < GRID_HEIGHT {
                                space += 10.0;
                            } else {
                                break;
                            }
                        }
                        if space > max_space {
                            max_space = space;
                            best_dir = *dir;
                        }
                    }
                    self.direction = best_dir;
                } else {
                    self.direction = safe_dirs[rng.gen_range(0..safe_dirs.len())];
                }
            }
        } else if rng.gen_range(0..100) < turn_chance {
            // Random turn occasionally for unpredictability
            let possible_dirs = match self.direction {
                Direction::Up | Direction::Down => vec![Direction::Left, Direction::Right],
                Direction::Left | Direction::Right => vec![Direction::Up, Direction::Down],
            };
            self.direction = possible_dirs[rng.gen_range(0..possible_dirs.len())];
        }
        
        // AI boost management
        if self.boost_energy > boost_threshold {
            // Use boost strategically
            if should_turn && rng.gen_range(0..100) < boost_chance * 10 {
                // Boost to escape danger
                self.is_boosting = true;
            } else if !should_turn && rng.gen_range(0..100) < boost_chance {
                // Occasional boost when safe
                self.is_boosting = true;
            } else {
                self.is_boosting = false;
            }
        } else {
            self.is_boosting = false;
        }
    }

    fn handle_input(&mut self, keycode: KeyCode, pressed: bool) {
        if !self.alive || self.player_type != PlayerType::Human {
            return;
        }

        // Handle boost
        if let Some(boost_key) = self.boost_key {
            if keycode == boost_key {
                if pressed && self.boost_energy > 10.0 {
                    self.is_boosting = true;
                } else {
                    self.is_boosting = false;
                }
            }
        }

        // Handle direction (only on key press)
        if pressed {
            if let Some((up, down, left, right)) = self.controls {
                let new_direction = if keycode == up {
                    Some(Direction::Up)
                } else if keycode == down {
                    Some(Direction::Down)
                } else if keycode == left {
                    Some(Direction::Left)
                } else if keycode == right {
                    Some(Direction::Right)
                } else {
                    None
                };

                if let Some(dir) = new_direction {
                    if !dir.is_opposite(&self.direction) {
                        self.direction = dir;
                    }
                }
            }
        }
    }
}

enum GameMode {
    Menu,
    Playing,
    Paused,
    GameOver { winner: String },
}

struct TrailParticle {
    position: Point2<f32>,
    velocity: Point2<f32>,
    lifetime: f32,
    color: Color,
}

impl TrailParticle {
    fn new(position: Point2<f32>, direction: Direction, color: Color) -> Self {
        let mut rng = rand::thread_rng();
        let base_vel = direction.to_velocity();
        TrailParticle {
            position,
            velocity: Point2 {
                x: -base_vel.0 * 0.5 + rng.gen_range(-10.0..10.0),
                y: -base_vel.1 * 0.5 + rng.gen_range(-10.0..10.0),
            },
            lifetime: rng.gen_range(0.2..0.5),
            color: Color::new(
                color.r,
                color.g,
                color.b,
                rng.gen_range(0.3..0.7),
            ),
        }
    }
    
    fn update(&mut self, dt: f32) {
        self.position.x += self.velocity.x * dt;
        self.position.y += self.velocity.y * dt;
        self.lifetime -= dt;
        self.velocity.x *= 0.95;
        self.velocity.y *= 0.95;
    }
}

struct GameState {
    cycles: Vec<LightCycle>,
    explosions: Vec<Explosion>,
    mode: GameMode,
    single_player: bool,
    ai_difficulty: AIDifficulty,
    screen_shake: f32,
    trail_particles: Vec<TrailParticle>,
}

impl GameState {
    fn new() -> Self {
        GameState {
            cycles: Vec::new(),
            explosions: Vec::new(),
            mode: GameMode::Menu,
            single_player: true,
            ai_difficulty: AIDifficulty::Medium,
            screen_shake: 0.0,
            trail_particles: Vec::new(),
        }
    }

    fn start_game(&mut self, single_player: bool) {
        self.cycles.clear();
        self.explosions.clear();
        self.trail_particles.clear();
        self.screen_shake = 0.0;
        self.single_player = single_player;
        
        // Player 1 (WASD controls, LShift for boost)
        self.cycles.push(LightCycle::new(
            200.0,
            GRID_HEIGHT / 2.0,
            Direction::Right,
            Color::from_rgb(0, 255, 255), // Cyan
            PlayerType::Human,
            Some((KeyCode::W, KeyCode::S, KeyCode::A, KeyCode::D)),
            Some(KeyCode::LShift),
            AIDifficulty::Medium, // Not used for human players
        ));

        // Player 2 or Computer (Arrow keys, RShift for boost)
        self.cycles.push(LightCycle::new(
            GRID_WIDTH - 200.0,
            GRID_HEIGHT / 2.0,
            Direction::Left,
            Color::from_rgb(255, 165, 0), // Orange
            if single_player { PlayerType::Computer } else { PlayerType::Human },
            if single_player { 
                None 
            } else { 
                Some((KeyCode::Up, KeyCode::Down, KeyCode::Left, KeyCode::Right))
            },
            if single_player { None } else { Some(KeyCode::RShift) },
            self.ai_difficulty,
        ));

        self.mode = GameMode::Playing;
    }

    fn check_game_over(&mut self) {
        let alive_count = self.cycles.iter().filter(|c| c.alive).count();
        
        if alive_count <= 1 {
            let winner = if alive_count == 0 {
                "Draw!".to_string()
            } else {
                let winner_idx = self.cycles.iter().position(|c| c.alive).unwrap();
                match winner_idx {
                    0 => "Player 1 Wins!".to_string(),
                    1 => if self.single_player { 
                        "Computer Wins!".to_string() 
                    } else { 
                        "Player 2 Wins!".to_string() 
                    },
                    _ => "Unknown".to_string(),
                }
            };
            self.mode = GameMode::GameOver { winner };
        }
    }
}

impl EventHandler for GameState {
    fn update(&mut self, _ctx: &mut Context) -> GameResult {
        match self.mode {
            GameMode::Playing => {
                let dt = 1.0 / 60.0;
                
                // AI updates
                let all_trails: Vec<_> = self.cycles.iter().map(|c| c.trail.clone()).collect();
                for (i, cycle) in self.cycles.iter_mut().enumerate() {
                    cycle.ai_update(&all_trails, i);
                }

                // Movement updates
                let all_trails: Vec<_> = self.cycles.iter().map(|c| c.trail.clone()).collect();
                for (i, cycle) in self.cycles.iter_mut().enumerate() {
                    let was_alive = cycle.alive;
                    cycle.update(dt, &all_trails, i);
                    
                    // Create explosion when cycle dies
                    if was_alive && !cycle.alive {
                        self.explosions.push(Explosion::new(cycle.position, cycle.color));
                        self.screen_shake = 20.0; // Add screen shake on collision
                    }
                    
                    // Create trail particles for boosting cycles
                    if cycle.alive && cycle.is_boosting {
                        let mut rng = rand::thread_rng();
                        if rng.gen_range(0..100) < 30 { // 30% chance to spawn particle
                            self.trail_particles.push(TrailParticle::new(
                                cycle.position,
                                cycle.direction,
                                cycle.color,
                            ));
                        }
                    }
                }

                // Update explosions
                for explosion in &mut self.explosions {
                    explosion.update(dt);
                }
                self.explosions.retain(|e| !e.is_finished());
                
                // Update trail particles
                for particle in &mut self.trail_particles {
                    particle.update(dt);
                }
                self.trail_particles.retain(|p| p.lifetime > 0.0);
                
                // Update screen shake
                if self.screen_shake > 0.0 {
                    self.screen_shake = (self.screen_shake - dt * 50.0).max(0.0);
                }

                self.check_game_over();
            }
            GameMode::Paused => {
                // Do nothing while paused
            }
            _ => {}
        }
        Ok(())
    }

    fn draw(&mut self, ctx: &mut Context) -> GameResult {
        let mut canvas = graphics::Canvas::from_frame(ctx, Color::BLACK);

        match &self.mode {
            GameMode::Menu => {
                let title_text = graphics::Text::new("LIGHT CYCLE");
                canvas.draw(
                    &title_text,
                    DrawParam::default()
                        .dest([GRID_WIDTH / 2.0 - 200.0, 300.0])
                        .color(Color::from_rgb(0, 255, 255))
                        .scale([4.0, 4.0]),
                );

                let sp_text = graphics::Text::new("Press 1 for Single Player");
                canvas.draw(
                    &sp_text,
                    DrawParam::default()
                        .dest([GRID_WIDTH / 2.0 - 120.0, 420.0])
                        .color(Color::WHITE),
                );

                let mp_text = graphics::Text::new("Press 2 for Two Players");
                canvas.draw(
                    &mp_text,
                    DrawParam::default()
                        .dest([GRID_WIDTH / 2.0 - 120.0, 460.0])
                        .color(Color::WHITE),
                );
                
                let diff_text = graphics::Text::new(format!("AI Difficulty: {:?} (Press D to change)", self.ai_difficulty));
                canvas.draw(
                    &diff_text,
                    DrawParam::default()
                        .dest([GRID_WIDTH / 2.0 - 160.0, 520.0])
                        .color(match self.ai_difficulty {
                            AIDifficulty::Easy => Color::from_rgb(100, 255, 100),
                            AIDifficulty::Medium => Color::from_rgb(255, 255, 100),
                            AIDifficulty::Hard => Color::from_rgb(255, 100, 100),
                        }),
                );

                let controls_text = graphics::Text::new("P1: WASD + LShift (boost) | P2: Arrows + RShift (boost)");
                canvas.draw(
                    &controls_text,
                    DrawParam::default()
                        .dest([GRID_WIDTH / 2.0 - 230.0, 600.0])
                        .color(Color::from_rgb(128, 128, 128)),
                );
            }
            GameMode::Playing => {
                // Apply screen shake
                let shake_offset = if self.screen_shake > 0.0 {
                    let mut rng = rand::thread_rng();
                    Point2 {
                        x: rng.gen_range(-self.screen_shake..self.screen_shake),
                        y: rng.gen_range(-self.screen_shake..self.screen_shake),
                    }
                } else {
                    Point2 { x: 0.0, y: 0.0 }
                };
                
                // Draw grid background with 8-bit style grid lines
                let mut mesh_builder = MeshBuilder::new();
                
                // Draw border with glow effect
                mesh_builder.rectangle(
                    DrawMode::stroke(3.0),
                    Rect::new(0.0, 0.0, GRID_WIDTH, GRID_HEIGHT),
                    Color::from_rgb(0, 100, 200),
                )?;
                
                // Draw vertical grid lines
                let grid_spacing = 50.0;
                let mut x = grid_spacing;
                while x < GRID_WIDTH {
                    mesh_builder.line(
                        &[Point2 { x, y: 0.0 }, Point2 { x, y: GRID_HEIGHT }],
                        1.0,
                        Color::from_rgba(20, 40, 60, 50),
                    )?;
                    x += grid_spacing;
                }
                
                // Draw horizontal grid lines
                let mut y = grid_spacing;
                while y < GRID_HEIGHT {
                    mesh_builder.line(
                        &[Point2 { x: 0.0, y }, Point2 { x: GRID_WIDTH, y }],
                        1.0,
                        Color::from_rgba(20, 40, 60, 50),
                    )?;
                    y += grid_spacing;
                }
                
                let grid_mesh = graphics::Mesh::from_data(ctx, mesh_builder.build());
                canvas.draw(&grid_mesh, DrawParam::default().dest(shake_offset));

                // Draw trails with glow effect
                for cycle in &self.cycles {
                    if cycle.trail.len() >= 2 {
                        let trail_vec: Vec<Point2<f32>> = cycle.trail.iter().copied().collect();
                        let mut mesh_builder = MeshBuilder::new();
                        
                        // Draw outer glow layer
                        for i in 0..trail_vec.len() - 1 {
                            let glow_color = Color::new(
                                cycle.color.r * 0.3,
                                cycle.color.g * 0.3,
                                cycle.color.b * 0.3,
                                0.3,
                            );
                            mesh_builder.line(
                                &[trail_vec[i], trail_vec[i + 1]],
                                CELL_SIZE * 2.5,
                                glow_color,
                            )?;
                        }
                        
                        // Draw main trail
                        for i in 0..trail_vec.len() - 1 {
                            mesh_builder.line(
                                &[trail_vec[i], trail_vec[i + 1]],
                                CELL_SIZE,
                                cycle.color,
                            )?;
                        }
                        
                        // Draw bright core
                        for i in 0..trail_vec.len() - 1 {
                            let core_color = Color::new(
                                (cycle.color.r * 1.2).min(1.0),
                                (cycle.color.g * 1.2).min(1.0),
                                (cycle.color.b * 1.2).min(1.0),
                                1.0,
                            );
                            mesh_builder.line(
                                &[trail_vec[i], trail_vec[i + 1]],
                                CELL_SIZE * 0.5,
                                core_color,
                            )?;
                        }
                        
                        let mesh = graphics::Mesh::from_data(ctx, mesh_builder.build());
                        canvas.draw(&mesh, DrawParam::default().dest(shake_offset));
                    }
                }
                
                // Draw trail particles
                for particle in &self.trail_particles {
                    let mesh = graphics::Mesh::from_data(
                        ctx,
                        MeshBuilder::new()
                            .circle(
                                DrawMode::fill(),
                                Point2 {
                                    x: particle.position.x + shake_offset.x,
                                    y: particle.position.y + shake_offset.y,
                                },
                                3.0 * particle.lifetime * 2.0,
                                0.5,
                                particle.color,
                            )?
                            .build(),
                    );
                    canvas.draw(&mesh, DrawParam::default());
                }

                // Draw cycles as 8-bit style vehicles
                for cycle in &self.cycles {
                    if cycle.alive {
                        let mut mesh_builder = MeshBuilder::new();
                        
                        // Calculate cycle orientation
                        let (body_width, body_height) = match cycle.direction {
                            Direction::Up | Direction::Down => (CYCLE_WIDTH, CYCLE_HEIGHT),
                            Direction::Left | Direction::Right => (CYCLE_HEIGHT, CYCLE_WIDTH),
                        };
                        
                        // Draw boost effect if active
                        if cycle.is_boosting {
                            // Draw boost trail particles
                            let boost_color = Color::new(
                                1.0,
                                0.8,
                                0.2,
                                0.5,
                            );
                            mesh_builder.circle(
                                DrawMode::fill(),
                                cycle.position,
                                body_width * 2.5,
                                0.1,
                                boost_color,
                            )?;
                        }
                        
                        // Draw large glow effect
                        let glow_intensity = if cycle.is_boosting { 0.6 } else { 0.4 };
                        let glow_size = if cycle.is_boosting { 2.0 } else { 1.5 };
                        let glow_color = Color::new(
                            cycle.color.r * glow_intensity,
                            cycle.color.g * glow_intensity,
                            cycle.color.b * glow_intensity,
                            0.2,
                        );
                        mesh_builder.circle(
                            DrawMode::fill(),
                            cycle.position,
                            body_width * glow_size,
                            0.1,
                            glow_color,
                        )?;
                        
                        // Draw main body (8-bit styled rectangle)
                        mesh_builder.rectangle(
                            DrawMode::fill(),
                            Rect::new(
                                cycle.position.x - body_width / 2.0,
                                cycle.position.y - body_height / 2.0,
                                body_width,
                                body_height,
                            ),
                            cycle.color,
                        )?;
                        
                        // Draw body outline for retro effect
                        mesh_builder.rectangle(
                            DrawMode::stroke(2.0),
                            Rect::new(
                                cycle.position.x - body_width / 2.0,
                                cycle.position.y - body_height / 2.0,
                                body_width,
                                body_height,
                            ),
                            Color::new(
                                (cycle.color.r * 1.3).min(1.0),
                                (cycle.color.g * 1.3).min(1.0),
                                (cycle.color.b * 1.3).min(1.0),
                                1.0,
                            ),
                        )?;
                        
                        // Draw cockpit/core as bright pixel
                        mesh_builder.rectangle(
                            DrawMode::fill(),
                            Rect::new(
                                cycle.position.x - 4.0,
                                cycle.position.y - 4.0,
                                8.0,
                                8.0,
                            ),
                            Color::WHITE,
                        )?;
                        
                        // Draw directional lights (8-bit style pixels)
                        let (light1_x, light1_y, light2_x, light2_y) = match cycle.direction {
                            Direction::Up => (
                                cycle.position.x - 6.0, cycle.position.y - body_height / 2.0 + 4.0,
                                cycle.position.x + 6.0, cycle.position.y - body_height / 2.0 + 4.0,
                            ),
                            Direction::Down => (
                                cycle.position.x - 6.0, cycle.position.y + body_height / 2.0 - 4.0,
                                cycle.position.x + 6.0, cycle.position.y + body_height / 2.0 - 4.0,
                            ),
                            Direction::Left => (
                                cycle.position.x - body_width / 2.0 + 4.0, cycle.position.y - 6.0,
                                cycle.position.x - body_width / 2.0 + 4.0, cycle.position.y + 6.0,
                            ),
                            Direction::Right => (
                                cycle.position.x + body_width / 2.0 - 4.0, cycle.position.y - 6.0,
                                cycle.position.x + body_width / 2.0 - 4.0, cycle.position.y + 6.0,
                            ),
                        };
                        
                        // Draw headlights as bright pixels
                        mesh_builder.rectangle(
                            DrawMode::fill(),
                            Rect::new(light1_x - 2.0, light1_y - 2.0, 4.0, 4.0),
                            Color::from_rgb(255, 255, 200),
                        )?;
                        mesh_builder.rectangle(
                            DrawMode::fill(),
                            Rect::new(light2_x - 2.0, light2_y - 2.0, 4.0, 4.0),
                            Color::from_rgb(255, 255, 200),
                        )?;
                        
                        let mesh = graphics::Mesh::from_data(ctx, mesh_builder.build());
                        canvas.draw(&mesh, DrawParam::default().dest(shake_offset));
                    }
                }

                // Draw explosions
                for explosion in &self.explosions {
                    for particle in &explosion.particles {
                        let alpha = (particle.lifetime / 1.5).min(1.0);
                        let color = Color::new(
                            particle.color.r,
                            particle.color.g,
                            particle.color.b,
                            particle.color.a * alpha,
                        );
                        
                        let mesh = graphics::Mesh::from_data(
                            ctx,
                            MeshBuilder::new()
                                .rectangle(
                                    DrawMode::fill(),
                                    Rect::new(
                                        particle.position.x - 2.0,
                                        particle.position.y - 2.0,
                                        4.0,
                                        4.0,
                                    ),
                                    color,
                                )?
                                .build(),
                        );
                        canvas.draw(&mesh, DrawParam::default().dest(shake_offset));
                    }
                }
                
                // Draw HUD
                let hud_text = "Press P to Pause | Press ESC to Quit";
                let hud = graphics::Text::new(hud_text);
                canvas.draw(
                    &hud,
                    DrawParam::default()
                        .dest([10.0, 10.0])
                        .color(Color::from_rgba(200, 200, 200, 180)),
                );
                
                // Draw boost energy bars
                for (i, cycle) in self.cycles.iter().enumerate() {
                    if cycle.alive && cycle.player_type == PlayerType::Human {
                        let bar_x = if i == 0 { 10.0 } else { GRID_WIDTH - 210.0 };
                        let bar_y = 40.0;
                        
                        // Draw background bar
                        let bg_mesh = graphics::Mesh::from_data(
                            ctx,
                            MeshBuilder::new()
                                .rectangle(
                                    DrawMode::stroke(2.0),
                                    Rect::new(bar_x, bar_y, 200.0, 20.0),
                                    Color::from_rgb(50, 50, 50),
                                )?
                                .build(),
                        );
                        canvas.draw(&bg_mesh, DrawParam::default());
                        
                        // Draw energy fill
                        let energy_width = (cycle.boost_energy / MAX_BOOST_ENERGY) * 196.0;
                        let energy_color = if cycle.is_boosting {
                            Color::from_rgb(255, 255, 100)
                        } else if cycle.boost_energy > 50.0 {
                            Color::from_rgb(0, 255, 100)
                        } else if cycle.boost_energy > 20.0 {
                            Color::from_rgb(255, 200, 0)
                        } else {
                            Color::from_rgb(255, 50, 50)
                        };
                        
                        if energy_width > 0.0 {
                            let energy_mesh = graphics::Mesh::from_data(
                                ctx,
                                MeshBuilder::new()
                                    .rectangle(
                                        DrawMode::fill(),
                                        Rect::new(bar_x + 2.0, bar_y + 2.0, energy_width, 16.0),
                                        energy_color,
                                    )?
                                    .build(),
                            );
                            canvas.draw(&energy_mesh, DrawParam::default());
                        }
                        
                        // Draw label
                        let label = if i == 0 { "P1 Boost" } else { "P2 Boost" };
                        let label_text = graphics::Text::new(label);
                        canvas.draw(
                            &label_text,
                            DrawParam::default()
                                .dest([bar_x, bar_y - 15.0])
                                .color(cycle.color)
                                .scale([0.8, 0.8]),
                        );
                    }
                }
            }
            GameMode::Paused => {
                // No shake in pause mode
                let shake_offset = Point2 { x: 0.0, y: 0.0 };
                
                // Draw the game state in background (dimmed)
                // First draw the game normally
                let mut mesh_builder = MeshBuilder::new();
                
                // Draw border
                mesh_builder.rectangle(
                    DrawMode::stroke(3.0),
                    Rect::new(0.0, 0.0, GRID_WIDTH, GRID_HEIGHT),
                    Color::from_rgb(0, 50, 100),
                )?;
                
                let grid_mesh = graphics::Mesh::from_data(ctx, mesh_builder.build());
                canvas.draw(&grid_mesh, DrawParam::default().dest(shake_offset));
                
                // Draw trails (dimmed)
                for cycle in &self.cycles {
                    if cycle.trail.len() >= 2 {
                        let trail_vec: Vec<Point2<f32>> = cycle.trail.iter().copied().collect();
                        let mut mesh_builder = MeshBuilder::new();
                        
                        for i in 0..trail_vec.len() - 1 {
                            let dimmed_color = Color::new(
                                cycle.color.r * 0.3,
                                cycle.color.g * 0.3,
                                cycle.color.b * 0.3,
                                0.5,
                            );
                            mesh_builder.line(
                                &[trail_vec[i], trail_vec[i + 1]],
                                CELL_SIZE,
                                dimmed_color,
                            )?;
                        }
                        
                        let mesh = graphics::Mesh::from_data(ctx, mesh_builder.build());
                        canvas.draw(&mesh, DrawParam::default());
                    }
                }
                
                // Draw pause overlay
                let overlay = graphics::Mesh::from_data(
                    ctx,
                    MeshBuilder::new()
                        .rectangle(
                            DrawMode::fill(),
                            Rect::new(0.0, 0.0, GRID_WIDTH, GRID_HEIGHT),
                            Color::from_rgba(0, 0, 0, 180),
                        )?
                        .build(),
                );
                canvas.draw(&overlay, DrawParam::default());
                
                // Draw pause text
                let pause_text = graphics::Text::new("PAUSED");
                canvas.draw(
                    &pause_text,
                    DrawParam::default()
                        .dest([GRID_WIDTH / 2.0 - 100.0, 350.0])
                        .color(Color::from_rgb(255, 255, 255))
                        .scale([3.0, 3.0]),
                );
                
                let resume_text = graphics::Text::new("Press P to Resume");
                canvas.draw(
                    &resume_text,
                    DrawParam::default()
                        .dest([GRID_WIDTH / 2.0 - 80.0, 450.0])
                        .color(Color::from_rgb(200, 200, 200)),
                );
                
                let quit_text = graphics::Text::new("Press ESC to Return to Menu");
                canvas.draw(
                    &quit_text,
                    DrawParam::default()
                        .dest([GRID_WIDTH / 2.0 - 120.0, 500.0])
                        .color(Color::from_rgb(200, 200, 200)),
                );
            }
            GameMode::GameOver { winner } => {
                let game_over_text = graphics::Text::new("GAME OVER");
                canvas.draw(
                    &game_over_text,
                    DrawParam::default()
                        .dest([GRID_WIDTH / 2.0 - 200.0, 350.0])
                        .color(Color::from_rgb(255, 0, 0))
                        .scale([3.0, 3.0]),
                );

                let winner_text = graphics::Text::new(winner.clone());
                canvas.draw(
                    &winner_text,
                    DrawParam::default()
                        .dest([GRID_WIDTH / 2.0 - 100.0, 450.0])
                        .color(Color::from_rgb(0, 255, 0))
                        .scale([1.5, 1.5]),
                );

                let restart_text = graphics::Text::new("Press ESC to return to menu");
                canvas.draw(
                    &restart_text,
                    DrawParam::default()
                        .dest([GRID_WIDTH / 2.0 - 120.0, 550.0])
                        .color(Color::WHITE),
                );
            }
        }

        canvas.finish(ctx)?;
        Ok(())
    }

    fn key_down_event(&mut self, _ctx: &mut Context, input: KeyInput, _repeat: bool) -> GameResult {
        if let Some(keycode) = input.keycode {
            match self.mode {
                GameMode::Menu => {
                    match keycode {
                        KeyCode::Key1 => self.start_game(true),
                        KeyCode::Key2 => self.start_game(false),
                        KeyCode::D => {
                            self.ai_difficulty = match self.ai_difficulty {
                                AIDifficulty::Easy => AIDifficulty::Medium,
                                AIDifficulty::Medium => AIDifficulty::Hard,
                                AIDifficulty::Hard => AIDifficulty::Easy,
                            };
                        }
                        _ => {}
                    }
                }
                GameMode::Playing => {
                    match keycode {
                        KeyCode::P => {
                            self.mode = GameMode::Paused;
                        }
                        KeyCode::Escape => {
                            self.mode = GameMode::Menu;
                        }
                        _ => {
                            for cycle in &mut self.cycles {
                                cycle.handle_input(keycode, true);
                            }
                        }
                    }
                }
                GameMode::Paused => {
                    match keycode {
                        KeyCode::P => {
                            self.mode = GameMode::Playing;
                        }
                        KeyCode::Escape => {
                            self.mode = GameMode::Menu;
                        }
                        _ => {}
                    }
                }
                GameMode::GameOver { .. } => {
                    if keycode == KeyCode::Escape {
                        self.mode = GameMode::Menu;
                    }
                }
            }
        }
        Ok(())
    }
    
    fn key_up_event(&mut self, _ctx: &mut Context, input: KeyInput) -> GameResult {
        if let Some(keycode) = input.keycode {
            if let GameMode::Playing = self.mode {
                for cycle in &mut self.cycles {
                    cycle.handle_input(keycode, false);
                }
            }
        }
        Ok(())
    }
}

fn main() -> GameResult {
    let cb = ContextBuilder::new("lightcycle", "TRON")
        .window_mode(ggez::conf::WindowMode::default()
            .dimensions(GRID_WIDTH, GRID_HEIGHT)
            .resizable(false));
    let (ctx, event_loop) = cb.build()?;
    let state = GameState::new();
    event::run(ctx, event_loop, state)
}
