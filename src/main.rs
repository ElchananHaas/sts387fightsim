#![feature(random)]
#![feature(int_roundings)]

use rand::{distr::weighted::WeightedIndex, prelude::*};
use std::{collections::HashMap, random::{DefaultRandomSource, Random}};

#[derive(Debug, PartialEq, Clone, Copy, Eq, PartialOrd, Ord, Hash)]
enum Card {
    Defend,
    DaggerThrow,
    WLP,
    CC,
    HeelHook,
    LegSweep,
    Expertise,
    Cost1Attack,
    Neutralize,
    App,
    PWail,
    Slimed,
    Unplayable,
    Void,
}

const STARTING_ENERGY: i32 = 5;
const NUM_CARDS: usize = Card::Void as usize + 1;
const NUM_FEATURES: usize = 22;
impl Card {
    fn energy(&self) -> Option<i32> {
        match self {
            Card::Defend => Some(1),
            Card::DaggerThrow => Some(1),
            Card::WLP => Some(1),
            Card::CC => Some(2),
            Card::HeelHook => Some(1),
            Card::LegSweep => Some(2),
            Card::Expertise => Some(1),
            Card::Cost1Attack => Some(1),
            Card::Neutralize => Some(0),
            Card::App => Some(1),
            Card::PWail => Some(1),
            Card::Unplayable => None,
            Card::Void => None,
            Card::Slimed => Some(1),
        }
    }

    fn block(&self) -> i32 {
        match self {
            Card::Defend => 9,
            Card::LegSweep => 13,
            _ => 0,
        }
    }

    fn is_attack(&self) -> bool {
        match self {
            Card::Defend => false,
            Card::DaggerThrow => true,
            Card::WLP => false,
            Card::CC => false,
            Card::HeelHook => true,
            Card::LegSweep => false,
            Card::Expertise => false,
            Card::Cost1Attack => true,
            Card::Neutralize => true,
            Card::App => false,
            Card::PWail => false,
            Card::Slimed => false,
            Card::Unplayable => false,
            Card::Void => false,
        }
    }

    fn weak(&self) -> i32 {
        match self {
            Card::CC => 2,
            Card::LegSweep => 2,
            Card::Neutralize => 1,
            _ => 0,
        }
    }
}

fn create_draw_pile() -> Vec<Card> {
    let mut draw_pile = vec![
        Card::Defend,
        Card::DaggerThrow,
        Card::WLP,
        Card::CC,
        Card::HeelHook,
        Card::LegSweep,
        Card::Expertise,
        Card::Cost1Attack, //DieDieDie
        Card::Neutralize,
        Card::Cost1Attack, //Strike
        Card::Defend,
        Card::Defend,
        Card::App,
        Card::PWail,
        Card::Slimed,
        Card::Unplayable, //Wound
        Card::Unplayable, //Dazed
        Card::Void,
    ];
    shuffle(&mut draw_pile);
    draw_pile
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct GameState {
    life: i32, //This is equal to HP+Block,
    weak: i32, //Number of stacks of weak.
    energy: i32,
    hand: Vec<Card>,
    deck: Vec<Card>,
    intangible: bool,
    heart_dmg: i32,
    attacks_played: i32,
}

impl GameState {
    fn observables(&self) -> GameState {
        let mut res = self.clone();
        res.deck.sort(); //TODO - can this be discarded to speed up the simulation?
        res.hand.sort(); //Hand is already sorted
        res
    }

    fn playable_actions(&self) -> Vec<usize> {
        let mut res: Vec<usize> = Vec::new();
        for i in 0..self.hand.len() {
            if let Some(energy) = self.hand[i].energy()
                && self.energy >= energy
            {
                res.push(i);
            }
        }
        res
    }

    fn play_card(&mut self, card_idx: usize) {
        let card = self.hand.swap_remove(card_idx);
        let energy = card.energy().expect("Card is playable");
        if energy > self.energy {
            panic!("Not enough energy to play card");
        }
        self.energy -= energy;
        self.intangible |= card == Card::App;
        self.life -= if self.intangible { 0 } else { 1 }; //Beat of Death
        self.life += card.block();
        self.attacks_played += if card.is_attack() { 1 } else { 0 };
        if self.attacks_played == 3 {
            self.attacks_played = 0;
            self.life += 4;
        }
        self.weak += card.weak();
        if self.weak > 0 && card == Card::HeelHook {
            self.energy += 1;
            self.draw();
        }
        if card == Card::PWail {
            self.heart_dmg -= 6;
        }
        if card == Card::Expertise {
            while self.hand.len() < 6 {
                self.draw();
            }
        }
        if card == Card::DaggerThrow {
            self.draw();
            self.discard();
        }
    }

    fn draw(&mut self) {
        //There is no way we are drawing through the entire deck this turn.
        let card = self.deck.pop().expect("Draw pile isn't empty");
        if card == Card::Void {
            self.energy = 0.max(self.energy - 1);
        }
        self.hand.push(card);
        self.hand.sort();
    }

    fn count(&self, card: Card) -> usize {
        (&self.hand).into_iter().filter(|c| **c == card).count()
    }
    fn in_hand(&self, card: Card) -> Option<usize> {
        for i in 0..self.hand.len() {
            if self.hand[i] == card {
                return Some(i);
            }
        }
        None
    }

    fn life_after_hit(&self) -> i32 {
        if self.intangible {
            self.life - 1
        } else {
            let modified_dmg = if self.weak > 0 {
                ((self.heart_dmg as f32) * 1.5 * 0.75) as i32
            } else {
                ((self.heart_dmg as f32) * 1.5) as i32
            };
            self.life - modified_dmg
        }
    }

    fn survive(&self) -> Option<i32> {
        let life = self.life_after_hit();
        if life > 0 { Some(life) } else { None }
    }

    fn discard(&mut self) {
        let worst = self
            .hand
            .iter()
            .enumerate()
            .min_by(|(_, card1), (_, card2)| {
                i32::cmp(
                    &self.dont_discard_score(**card1),
                    &self.dont_discard_score(**card2),
                )
            });
        self.hand.swap_remove(worst.expect("Hand is nonempty").0);
    }

    //Discard is very situational and this is sometimes a bad ordering.
    fn dont_discard_score(&self, card: Card) -> i32 {
        match card {
            Card::Defend => 50,
            Card::DaggerThrow => 40,
            Card::WLP => 20,
            Card::CC => 30,
            Card::HeelHook => 10,
            Card::LegSweep => 70,
            Card::Expertise => 20,
            Card::Cost1Attack => 5,
            Card::Neutralize => 25,
            Card::App => 100,
            Card::PWail => 15,
            Card::Slimed => 2,
            Card::Unplayable => 1,
            Card::Void => 3,
        }
    }
}


#[derive(Clone, Debug, PartialEq,)]
struct QEntry {
    taken: f32,
    reward_sum: f32,
}

struct MctsEntry {
    //This could be bad if an action is taken over 2^24 = 16,000,000 times. But thats very big.
    visit_count: f32,
    q_vals: Vec<QEntry>,
}

impl MctsEntry {
    fn ucb(&self) -> usize {
        for i in 0..self.q_vals.len() {
            if self.q_vals[i].taken == 0.0 {
                return i
            }
        }
        let ucb_action = self.q_vals.iter().map(|q| {
            let mean = q.reward_sum / q.taken;
            let ucb_adjust = f32::sqrt(EXPLORE_FACTOR * f32::ln(self.visit_count) / q.taken);
            mean + ucb_adjust
        }).enumerate()
          .max_by(|(_, a), (_, b)| a.total_cmp(b))
          .map(|(index, _)| index)
          .expect("Non-empty list of actions");
        ucb_action
    }

    fn update(&mut self, action: usize, reward: f32) {
        self.visit_count += 1.0;
        self.q_vals[action].taken += 1.0;
        self.q_vals[action].reward_sum += reward;
    }
}

fn mcts(f: impl Fn() -> GameState) {
    let mut total_reward = 0.0;
    let mut value_map: HashMap<GameState, MctsEntry> = HashMap::new();
    for i in 0..10000000 {
        let mut state = f();
        let reward = mcts_rollout(&mut state, &mut value_map);
        total_reward += reward;
        if i % 1000 == 0 {
            println!("Average rewards are {}", total_reward/(i as f32));
        }
    }
}
//This function rolls out a game. It mutatates its input
fn mcts_rollout(state: &mut GameState, value_map: &mut HashMap<GameState, MctsEntry>) -> f32 { 
    let mut states = Vec::new();
    let mut taken_actions = Vec::new();
    let reward: f32;
    loop {
        states.push(state.observables());
        let mut actions = state.playable_actions();
        actions.push(NUM_CARDS);
        let mcts_entry = value_map.entry(state.observables())
        .or_insert_with(|| MctsEntry {
            visit_count: 0.0,
            q_vals: vec![QEntry { taken: 0.0, reward_sum: 0.0 } ;actions.len()],
        });
        let action_idx = mcts_entry.ucb();
        taken_actions.push(action_idx);
        if action_idx == actions.len() - 1 {
            reward = if state.survive().is_some() {1f32} else {0f32};
            break; 
        }
        state.play_card(actions[action_idx]);
    }
    for i in 0..states.len() {
        value_map.get_mut(&states[i]).expect("State found").update(taken_actions[i], reward);
    }
    reward
}

const LR: f32 = 0.003;
const EXPLORE_FACTOR: f32 = 0.5;

//This assumes we have already Gambled.
fn simulate(state: &mut GameState) -> bool {
    loop {
        if let Some(i) = state.in_hand(Card::App)
            && state.energy >= 1
        {
            state.play_card(i);
            continue;
        }
        //If the heart is already weak and we have 2 defends and 2 energy its better than a Leg Sweep.
        if state.weak > 0 && state.count(Card::Defend) >= 2 && state.energy == 2 {
            let i = state.in_hand(Card::Defend).expect("Defend is in hand");
            state.play_card(i);
            let i = state.in_hand(Card::Defend).expect("Defend is in hand");
            state.play_card(i);
            continue;
        }
        //Prioritize leg sweep for the block and weak.
        if let Some(i) = state.in_hand(Card::LegSweep)
            && state.energy >= 2
        {
            state.play_card(i);
            continue;
        }
        //Play neutralize to land weak
        if state.weak == 0
            && let Some(i) = state.in_hand(Card::Neutralize)
        {
            state.play_card(i);
            continue;
        }
        //Play CC to land weak
        if state.weak == 0
            && state.energy >= 2
            && let Some(i) = state.in_hand(Card::CC)
        {
            state.play_card(i);
            continue;
        }
        if state.energy > 1
            && let Some(i) = state.in_hand(Card::Defend)
        {
            state.play_card(i);
            continue;
        }
        if state.energy >= 1
            && let Some(i) = state.in_hand(Card::DaggerThrow)
        {
            state.play_card(i);
            continue;
        }
        if let Some(i) = state.in_hand(Card::Neutralize) {
            state.play_card(i);
            continue;
        }
        if state.energy >= 1
            && let Some(i) = state.in_hand(Card::Expertise)
        {
            state.play_card(i);
            continue;
        }
        if state.energy >= 1
            && state.attacks_played % 3 == 2
            && let Some(i) = state.in_hand(Card::Cost1Attack)
        {
            state.play_card(i);
            continue;
        }
        break;
    }
    state.survive().is_some()
}

fn dash_state() -> GameState {
    let mut state = GameState {
        life: 51,
        weak: 0,
        energy: STARTING_ENERGY - 2,
        hand: vec![],
        deck: create_draw_pile(),
        intangible: false,
        heart_dmg: 46,
        attacks_played: 0,
    };
    for _ in 0..4 {
        state.draw();
    }
    state
}

fn gamble_state() -> GameState {
    let mut state = GameState {
        life: 40,
        weak: 0,
        energy: STARTING_ENERGY,
        hand: vec![],
        deck: create_draw_pile(),
        intangible: false,
        heart_dmg: 46,
        attacks_played: 0,
    };
    for _ in 0..5 {
        state.draw();
    }
    state
}
fn main() {
    /*let gamble_line = score(|| {
        let mut state = gamble_state();
        simulate(&mut state)
    });
    println!("Gamble line is {}", gamble_line);
    let dash_line = score(|| {
        let mut state = dash_state();
        simulate(&mut state)
    });
    println!("Dash line is {}", dash_line);*/
    mcts(dash_state);
}

fn score(mut f: impl FnMut(usize) -> bool) -> f32 {
    let mut wins = 0;
    let count = 10000;
    for c in 0..count {
        wins += if f(c) { 1 } else { 0 };
    }
    (wins as f32) / (count as f32)
}
fn shuffle<T>(vec: &mut [T]) {
    for i in 1..vec.len() {
        let rand = usize::random(&mut DefaultRandomSource);
        let j = (rand as usize) % (i + 1);
        vec.swap(i, j);
    }
}
