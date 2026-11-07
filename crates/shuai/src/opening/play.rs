//! The match-scoped event and deadline loop.

use std::future::{Future, pending};
use std::pin::Pin;
use std::time::Duration;

use miette::{IntoDiagnostic, Report, miette};
use shuai_client::prelude::*;
use shuai_cpsat::opening::{self, Outcome, Path, Solution};
use sioc::prelude::*;
use tokio::task::JoinError;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::instrument;

use super::execution::{Execution, Progress, Step};
use super::problem::{build_problem, check_supported, plan_is_applicable, solve_options};
use super::solve::OpeningSolve;
use crate::game::{GameState, PlayerId, Terrain};
use crate::session::{MatchError, next_event};

const SOLVE_BUDGET: Duration = Duration::from_millis(5700);
const GAME_OVER_TIMEOUT: Duration = Duration::from_secs(10);
const SOLVER_GRACE: Duration = Duration::from_secs(5);
const REACTION_TARGET: Duration = Duration::from_millis(300);

/// The time limits of one Opening match.
#[derive(Debug, Clone, Copy)]
pub struct OpeningConfig {
    /// The solver's budget, which also sets the earliest possible first path.
    pub solve_budget: Duration,
    /// How long to wait for `game_over` once the match should be ending.
    pub game_over_timeout: Duration,
    /// How long to wait for a stopped solver before reporting it.
    pub solver_grace: Duration,
}

impl Default for OpeningConfig {
    fn default() -> Self {
        Self {
            solve_budget: SOLVE_BUDGET,
            game_over_timeout: GAME_OVER_TIMEOUT,
            solver_grace: SOLVER_GRACE,
        }
    }
}

/// Where one match's opening stands.
enum Stage {
    /// No map has arrived, so there is nothing to solve.
    AwaitingMap,
    /// The solver is running while events keep flowing.
    Solving(OpeningSolve),
    /// The plan is being submitted and reconciled.
    Executing(Execution),
    /// No more attacks. Reading until `game_over`, up to the deadline.
    AwaitingGameOver { deadline: Instant },
}

/// The one local send that may be in progress, so backpressure cannot stop event reads.
type PendingSend<'a> = Option<Pin<Box<dyn Future<Output = sioc::error::Result<()>> + Send + 'a>>>;

/// Plays one match with the default limits. See [`play_opening_with`].
///
/// # Errors
/// See [`play_opening_with`].
pub async fn play_opening(
    start: Box<GameStartData>,
    rx: &mut SocketReceiver,
    tx: &GeneralsIoSender,
    shutdown: &CancellationToken,
) -> Result<(), MatchError> {
    play_opening_with(OpeningConfig::default(), start, rx, tx, shutdown).await
}

/// Solves, executes and surrenders the opening of one match.
///
/// Returns `Ok(())` after `game_over`, or when `shutdown` is requested. Every exit
/// stops the solver and observes that it ended before returning.
///
/// # Errors
/// Returns a classified [`MatchError`] when the opening cannot continue.
#[instrument(skip_all, err(Debug))]
pub async fn play_opening_with(
    config: OpeningConfig,
    start: Box<GameStartData>,
    rx: &mut SocketReceiver,
    tx: &GeneralsIoSender,
    shutdown: &CancellationToken,
) -> Result<(), MatchError> {
    let mut stage = Stage::AwaitingMap;
    let mut reactions = Reactions::default();
    let result = drive(config, start, rx, tx, shutdown, &mut stage, &mut reactions).await;
    if let Stage::Solving(solve) = stage {
        solve.reap(config.solver_grace).await;
    }
    reactions.log();
    result
}

async fn drive(
    config: OpeningConfig,
    start: Box<GameStartData>,
    rx: &mut SocketReceiver,
    tx: &GeneralsIoSender,
    shutdown: &CancellationToken,
    stage: &mut Stage,
    reactions: &mut Reactions,
) -> Result<(), MatchError> {
    check_supported(&start).map_err(abandon)?;
    let mut game = GameState::default();
    game.start(start)
        .into_diagnostic()
        .map_err(MatchError::Fatal)?;
    let mut pending_send: PendingSend<'_> = None;
    let mut last_update = Instant::now();
    let mut eliminated = false;
    loop {
        let game_over_deadline = match stage {
            Stage::AwaitingGameOver { deadline } => Some(*deadline),
            _ => None,
        };
        tokio::select! {
            biased;
            () = shutdown.cancelled() => return Ok(()),
            event = next_event(rx) => match event? {
                GeneralsIoEvent::GameOver(_) => return Ok(()),
                GeneralsIoEvent::GameUpdate(Event { payload, .. }) => {
                    last_update = Instant::now();
                    game
                        .update(payload.data)
                        .into_diagnostic()
                        .map_err(MatchError::Fatal)?;
                    if matches!(stage, Stage::AwaitingMap) {
                        let problem = build_problem(&game, config.solve_budget).map_err(abandon)?;
                        let options = solve_options(config.solve_budget);
                        *stage = Stage::Solving(OpeningSolve::start(problem, options));
                    }
                }
                GeneralsIoEvent::GameLost(Event { payload, .. }) => {
                    tracing::info!(?payload.data, "local player eliminated");
                    eliminated = true;
                    end_opening(stage, config).await;
                }
                GeneralsIoEvent::GameWon(_) => {
                    tracing::info!("local player won");
                    end_opening(stage, config).await;
                }
                GeneralsIoEvent::GameStart(_) => {
                    return Err(MatchError::Fatal(miette!("game_start during an active match")));
                }
                _ => {}
            },
            result = solver_finished(stage) => {
                // The worker has ended, so an error below must leave nothing to reap.
                *stage = Stage::AwaitingMap;
                *stage = Stage::Executing(accept_outcome(result, &game)?);
            }
            result = send_finished(&mut pending_send) => {
                pending_send = None;
                result.map_err(|error| MatchError::Fatal(Report::new(error)))?;
                reactions.record(last_update.elapsed());
            }
            () = sleep_until_deadline(game_over_deadline) => {
                return Err(if eliminated {
                    MatchError::Eliminated(miette!("game_over did not follow game_lost"))
                } else {
                    abandon(miette!("game_over did not arrive"))
                });
            }
        }
        if pending_send.is_none()
            && let Stage::Executing(execution) = stage
            && let Some(step) = observe(execution, &game)
        {
            match step {
                Step::Wait => {}
                Step::Attack(attack) => {
                    let from = i32::try_from(attack.from).expect("map tiles fit the wire type");
                    let to = i32::try_from(attack.to).expect("map tiles fit the wire type");
                    tracing::debug!(from, to, attack.attack_index, "submitting attack");
                    pending_send = Some(Box::pin(tx.attack(from, to, false, attack.attack_index)));
                }
                Step::Complete => {
                    tracing::info!("opening completed, surrendering");
                    pending_send = Some(Box::pin(tx.surrender()));
                    *stage = Stage::AwaitingGameOver {
                        deadline: Instant::now() + config.game_over_timeout,
                    };
                }
                Step::Deviated(deviation) => {
                    return Err(abandon(miette!("plan deviated: {deviation:?}")));
                }
            }
        }
    }
}

fn observe(execution: &mut Execution, game: &GameState) -> Option<Step> {
    let observation = game.observation()?;
    let map = game.map()?;
    let player = PlayerId::try_from(game.metadata()?.player_index).ok()?;
    let width = map.view().armies.width();
    Some(execution.observe(&Progress {
        turn: observation.turn,
        attack_index: observation.attack_index,
        owned: |tile: usize| {
            map.terrain(tile / width, tile % width) == Some(Terrain::Player(player))
        },
    }))
}

/// Ends attacks and any solve, and starts the deadline for `game_over` unless the
/// opening's own surrender already started it.
async fn end_opening(stage: &mut Stage, config: OpeningConfig) {
    if matches!(stage, Stage::AwaitingGameOver { .. }) {
        return;
    }
    let deadline = Instant::now() + config.game_over_timeout;
    if let Stage::Solving(solve) = std::mem::replace(stage, Stage::AwaitingGameOver { deadline }) {
        solve.reap(config.solver_grace).await;
    }
}

async fn solver_finished(stage: &mut Stage) -> Result<Result<Outcome, opening::Error>, JoinError> {
    match stage {
        Stage::Solving(solve) => solve.finished().await,
        _ => pending().await,
    }
}

async fn send_finished(pending_send: &mut PendingSend<'_>) -> sioc::error::Result<()> {
    match pending_send {
        Some(send) => send.as_mut().await,
        None => pending().await,
    }
}

async fn sleep_until_deadline(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => pending().await,
    }
}

/// Accepts a solved plan if it still applies to the latest map.
fn accept_outcome(
    result: Result<Result<Outcome, opening::Error>, JoinError>,
    game: &GameState,
) -> Result<Execution, MatchError> {
    let outcome = result
        .map_err(|error| MatchError::Fatal(miette!("solver worker failed: {error}")))?
        .map_err(|error| match error {
            opening::Error::Input(_) => abandon(error),
            error => MatchError::Fatal(miette!("solver failed: {error}")),
        })?;
    let solution = match outcome {
        Outcome::Optimal { solution, timings } => {
            tracing::info!(objective = solution.objective, ?timings, "solved optimally");
            solution
        }
        Outcome::Feasible { solution, timings } => {
            tracing::info!(objective = solution.objective, ?timings, "solved feasibly");
            solution
        }
        Outcome::Infeasible { timings } => {
            return Err(abandon(miette!("no feasible opening, after {timings:?}")));
        }
        Outcome::NoIncumbent { status, timings } => {
            return Err(abandon(miette!(
                "no incumbent opening: {status:?}, after {timings:?}"
            )));
        }
        Outcome::BudgetExhausted { timings } => {
            return Err(abandon(miette!(
                "solve budget exhausted, after {timings:?}"
            )));
        }
    };
    let Solution { paths, .. } = &solution;
    check_applicable(paths, game)?;
    Ok(Execution::new(paths))
}

fn check_applicable(paths: &[Path], game: &GameState) -> Result<(), MatchError> {
    let applicable = game.map().is_some_and(|map| {
        game.metadata()
            .and_then(|start| PlayerId::try_from(start.player_index).ok())
            .is_some_and(|player| {
                plan_is_applicable(paths.iter().flat_map(|path| &path.vertices), map, player)
            })
    });
    if applicable {
        Ok(())
    } else {
        Err(abandon(miette!("solved plan no longer fits the map")))
    }
}

fn abandon(reason: impl std::fmt::Display) -> MatchError {
    MatchError::Abandon(miette!("{reason}"))
}

/// Measures local reaction: from an update's receipt to the local send being accepted.
#[derive(Debug, Default)]
struct Reactions {
    count: u32,
    missed_target: u32,
    slowest: Duration,
}

impl Reactions {
    fn record(&mut self, reaction: Duration) {
        self.count += 1;
        self.slowest = self.slowest.max(reaction);
        if reaction > REACTION_TARGET {
            self.missed_target += 1;
            tracing::warn!(?reaction, "local reaction missed its target");
        }
    }

    fn log(&self) {
        tracing::info!(
            count = self.count,
            missed_target = self.missed_target,
            slowest = ?self.slowest,
            "local reaction summary"
        );
    }
}
