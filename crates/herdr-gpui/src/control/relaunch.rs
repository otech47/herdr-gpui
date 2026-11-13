//! `herdr-gpui relaunch countdown`: shows the countdown in every window and
//! answers once it runs out or the user cancels it. The caller quits and
//! reopens the app itself.
#[cfg(unix)]
use super::{protocol::ErrorCode, socket::Incoming};
use super::{
    protocol::{RelaunchRequest, Request, Response},
    rejected, report, send,
};
#[cfg(unix)]
use crate::relaunch_notice::{Notice, Outcome};
use crate::{cli::RelaunchCommand, relaunch_notice::MAX_SECONDS};
#[cfg(unix)]
use gpui::App;
use std::process::ExitCode;

const EXIT_CANCELLED: u8 = 4;
#[cfg(unix)]
const MAX_WAITING: usize = 8;

const USAGE: &str = "\
herdr-gpui relaunch countdown [--seconds N] [--reason TEXT]
    Shows a countdown with a Cancel button in the middle of every Herdr
    window. Prints proceed when it runs out and cancelled when the user
    cancels it. It lasts N seconds, 5 by default and at most 30. TEXT says
    what the relaunch installs. The command only asks: the caller quits and
    reopens the app. A second caller during a countdown shares it.
Exit status: 0 proceed, 1 refused, 2 invalid arguments, 3 Herdr GPUI not running, 4 cancelled.";

#[cfg(unix)]
pub(super) fn ask(
    request: &RelaunchRequest,
    incoming: Incoming,
    waiting: &mut Vec<Incoming>,
    cx: &mut App,
) {
    if waiting.len() >= MAX_WAITING {
        incoming.respond(Response::error(
            ErrorCode::Busy,
            "Too many callers are waiting on a relaunch",
        ));
        return;
    }
    // With no window open there is nobody to ask.
    if super::main_windows(cx).is_empty() {
        incoming.respond(Response::Relaunch { proceed: true });
        return;
    }
    Notice::start(request.seconds, request.reason.as_deref(), cx);
    waiting.push(incoming);
}

#[cfg(unix)]
pub(super) fn serve(waiting: &mut Vec<Incoming>, cx: &mut App) {
    let Some(outcome) = Notice::tick(cx) else {
        return;
    };
    let proceed = outcome == Outcome::Proceed;
    tracing::info!(proceed, callers = waiting.len(), "Relaunch countdown ended");
    for incoming in waiting.drain(..) {
        incoming.respond(Response::Relaunch { proceed });
    }
}

pub(crate) fn run(command: RelaunchCommand) -> ExitCode {
    let (seconds, reason) = match command {
        RelaunchCommand::Help => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        RelaunchCommand::Countdown { seconds, reason } => (seconds, reason),
    };
    let request = Request::Relaunch(RelaunchRequest {
        seconds: seconds.min(MAX_SECONDS),
        reason,
    });
    match send(&request) {
        Ok(Response::Relaunch { proceed: true }) => {
            println!("proceed");
            ExitCode::SUCCESS
        }
        Ok(Response::Relaunch { proceed: false }) => {
            println!("cancelled");
            ExitCode::from(EXIT_CANCELLED)
        }
        Ok(other) => report(Err(rejected(other))),
        Err(error) => report(Err(error)),
    }
}
