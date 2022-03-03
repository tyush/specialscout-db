use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
pub struct DetailedTeam {
    pub team: Team,
    pub matches_played: u16,
    pub matches_won: u16,
    pub balls_thrown: u16,
    pub balls_sunk_lower: u16,
    pub balls_sunk_upper: u16,
    pub def: f32,
    pub driv: f32,
    pub conf: f32, 
    pub avg_score: f32,
    pub rp: i16,
    pub prev_points: i16,
    pub est_points: i16
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Deserialize, Serialize)]
#[serde(from = "u16")]
#[repr(transparent)]
pub struct Team {
    number: u16,
}

impl From<u16> for Team {
    fn from(x: u16) -> Self {
        Team { number: x }
    }
}


#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum FormIngest {
    Match {
        timestamp: i32,
        event: String,
        match_number: i16,
        team_number: u32,

        did_preload: bool,
        did_taxi: bool,
        got_field_cargo: bool,
        did_hp_shot: bool,
        did_hp_sink: bool,
        auto_scored_lower: i16,
        auto_scored_upper: i16,
        auto_shots: i16,

        teleop_scored_lower: i16,
        teleop_scored_upper: i16,
        teleop_shots: i16,

        pins: i16,
        times_pinned: i16,
        penalties: i16,

        climb: i8,
        performance: i16,

        comments: String,
        red_score: i32,
        blue_score: i32,
    },
    Pit {
        time_stamp: i32,
        team_name: String,
        team_number: i32,
        drivetrain: String,
        weight: u16,

        size: Size,

        can_shoot_auto_upper: bool,
        can_shoot_auto_lower: bool,
        can_shoot_teleop_upper: bool,
        can_shoot_teleop_lower: bool,

        climb: i8,

        comment: String,
        build_quality: i16,
        driver_team: i16,
        confidence: i16,
        picture: String
    }
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug)]
pub struct Size { pub x: f32, pub y: f32, pub z: f32 }

pub fn sim_score(did_taxi: bool, auto_scored_upper: i16, auto_scored_lower: i16, teleop_scored_upper: i16, teleop_scored_lower: i16, climb: i8) -> i64 {
    let mut accum: i64 = 0;

    if did_taxi {
        accum += 2;
    }

    accum += (auto_scored_upper as i64) * 4;
    accum += (auto_scored_lower as i64) * 2;
    
    accum += (teleop_scored_upper as i64) * 2;
    accum += (teleop_scored_lower as i64) * 1;

    accum += match climb {
        -1 => 0,
        0 => 4,
        1 => 6,
        2 => 10,
        3 => 15,
        _ => 0
    };

    accum
}