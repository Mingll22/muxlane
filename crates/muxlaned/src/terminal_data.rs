use std::{
    io::{self, BufRead, Write},
    sync::{Arc, Mutex},
};

use muxlane_core::{
    CoreError,
    layout::{Layout, validate_id},
    storage::Storage,
    terminal,
};
use muxlane_protocol::{
    AttachedTerminal, MAX_TERMINAL_DATA_MESSAGE_BYTES, Phase3Error, Phase3Event, Phase3Frame,
    Phase3Request, Phase3Response, TERMINAL_DATA_PROTOCOL_MAJOR, TERMINAL_DATA_PROTOCOL_MINOR,
    TerminalDataError, TerminalDataEvent, TerminalDataFrame, TerminalDataRequest,
    TerminalDataRequestEnvelope, TerminalDataResponse, TerminalDataResult, TerminalStream,
};

use crate::phase3::{Gateway, SharedWriter};

const FORMAL_TMUX_SOCKET: &str = "muxlane-runtime";
const FORMAL_SESSION_PREFIX: &str = "muxlane-";

#[derive(Clone)]
struct Route {
    project_id: String,
    terminal_id: String,
    project_key: String,
    window_id: String,
}

struct FormalWriter {
    output: io::Stdout,
    pending: Vec<u8>,
    route: Arc<Mutex<Option<Route>>>,
}

impl Write for FormalWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.pending.extend_from_slice(bytes);
        while let Some(end) = self.pending.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=end).collect();
            match serde_json::from_slice::<Phase3Frame>(&line[..line.len() - 1]) {
                Ok(Phase3Frame::Event { event }) => {
                    if let Some(event) = map_event(event, &self.route) {
                        serde_json::to_writer(
                            &mut self.output,
                            &TerminalDataFrame::Event { event },
                        )?;
                        self.output.write_all(b"\n")?;
                    }
                }
                Ok(Phase3Frame::Response { .. }) => {}
                Err(_) => self.output.write_all(&line)?,
            }
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }
}

pub fn run_gateway() -> Result<(), CoreError> {
    let storage = Storage::open(Layout::discover()?)?;
    let route = Arc::new(Mutex::new(None));
    let output: SharedWriter = Arc::new(Mutex::new(Box::new(FormalWriter {
        output: io::stdout(),
        pending: Vec::new(),
        route: Arc::clone(&route),
    })));
    let mut gateway =
        Gateway::new_with_prefix(FORMAL_TMUX_SOCKET.to_owned(), FORMAL_SESSION_PREFIX.to_owned())
            .map_err(core_from_phase3)?;
    let mut handshaken = false;
    for line in io::stdin().lock().lines() {
        let line = line.map_err(CoreError::io)?;
        if line.len() > MAX_TERMINAL_DATA_MESSAGE_BYTES {
            return Err(CoreError::new("INVALID_REQUEST", "Terminal data message exceeds limit"));
        }
        let envelope: TerminalDataRequestEnvelope = serde_json::from_str(&line)
            .map_err(|_| CoreError::new("INVALID_REQUEST", "invalid Terminal data request"))?;
        let result = if !handshaken
            && !matches!(envelope.request, TerminalDataRequest::Handshake { .. })
        {
            Err(TerminalDataError {
                code: "PROTOCOL_INCOMPATIBLE".to_owned(),
                message: "terminal.handshake must be the first request".to_owned(),
            })
        } else {
            dispatch(&storage, &mut gateway, &output, &route, envelope.request).map(|response| {
                if matches!(response, TerminalDataResponse::Handshake { .. }) {
                    handshaken = true;
                }
                response
            })
        };
        emit_response(&output, envelope.id, result)?;
    }
    gateway.disconnect().map_err(core_from_phase3)?;
    Ok(())
}

fn dispatch(
    storage: &Storage,
    gateway: &mut Gateway,
    output: &SharedWriter,
    route_state: &Arc<Mutex<Option<Route>>>,
    request: TerminalDataRequest,
) -> Result<TerminalDataResponse, TerminalDataError> {
    match request {
        TerminalDataRequest::Handshake { protocol_major, protocol_minor: _, client_name } => {
            if protocol_major != TERMINAL_DATA_PROTOCOL_MAJOR
                || client_name.is_empty()
                || client_name.len() > 64
                || client_name.chars().any(char::is_control)
            {
                return Err(formal_error(
                    "PROTOCOL_INCOMPATIBLE",
                    "Terminal data protocol is incompatible",
                ));
            }
            let response = gateway.handle(Phase3Request::Probe, output).map_err(map_error)?;
            match response {
                Phase3Response::Probe { connection_id, tmux_version } => {
                    Ok(TerminalDataResponse::Handshake {
                        protocol_major: TERMINAL_DATA_PROTOCOL_MAJOR,
                        protocol_minor: TERMINAL_DATA_PROTOCOL_MINOR,
                        connection_id,
                        tmux_version,
                        max_message_bytes: MAX_TERMINAL_DATA_MESSAGE_BYTES,
                    })
                }
                _ => Err(formal_error(
                    "INTERNAL_ERROR",
                    "Terminal engine returned an invalid handshake",
                )),
            }
        }
        TerminalDataRequest::Attach { terminal_id } => {
            attach(storage, gateway, output, route_state, &terminal_id)
        }
        TerminalDataRequest::Switch { terminal_id } => {
            if let Some(current) = route_state
                .lock()
                .map_err(|_| formal_error("INTERNAL_ERROR", "Terminal route is unavailable"))?
                .clone()
            {
                if let Phase3Response::State { attached: Some(stream) } =
                    gateway.handle(Phase3Request::ReadState, output).map_err(map_error)?
                {
                    gateway.handle(Phase3Request::Detach { stream }, output).map_err(map_error)?;
                }
                if current.terminal_id == terminal_id {
                    *route_state.lock().map_err(|_| {
                        formal_error("INTERNAL_ERROR", "Terminal route is unavailable")
                    })? = None;
                }
            }
            attach(storage, gateway, output, route_state, &terminal_id)
        }
        TerminalDataRequest::StartStream { stream } => {
            let phase = to_phase_stream(&stream, route_state)?;
            match gateway
                .handle(Phase3Request::StartStream { stream: phase }, output)
                .map_err(map_error)?
            {
                Phase3Response::StreamStarted { .. } => {
                    Ok(TerminalDataResponse::StreamStarted { stream })
                }
                _ => Err(formal_error(
                    "INTERNAL_ERROR",
                    "Terminal engine returned an invalid response",
                )),
            }
        }
        TerminalDataRequest::Detach { stream } => {
            let phase = to_phase_stream(&stream, route_state)?;
            gateway.handle(Phase3Request::Detach { stream: phase }, output).map_err(map_error)?;
            *route_state
                .lock()
                .map_err(|_| formal_error("INTERNAL_ERROR", "Terminal route is unavailable"))? =
                None;
            Ok(TerminalDataResponse::Detached)
        }
        TerminalDataRequest::SendInput { stream, bytes } => {
            let phase = to_phase_stream(&stream, route_state)?;
            gateway
                .handle(Phase3Request::SendInput { stream: phase, bytes }, output)
                .map_err(map_error)?;
            Ok(TerminalDataResponse::Acknowledged)
        }
        TerminalDataRequest::Resize { stream, columns, rows } => {
            let phase = to_phase_stream(&stream, route_state)?;
            gateway
                .handle(Phase3Request::Resize { stream: phase, columns, rows }, output)
                .map_err(map_error)?;
            Ok(TerminalDataResponse::Acknowledged)
        }
        TerminalDataRequest::Close { terminal_id } => {
            validate_id(&terminal_id).map_err(map_core_error)?;
            let terminal_record = storage.terminal(&terminal_id).map_err(map_core_error)?;
            let route = route_for(storage, &terminal_record).map_err(map_core_error)?;
            gateway
                .handle(
                    Phase3Request::CloseWindow {
                        project_id: route.project_key.clone(),
                        window_id: route.window_id.clone(),
                    },
                    output,
                )
                .map_err(map_error)?;
            storage.close_terminal(&terminal_id).map_err(map_core_error)?;
            if route_state
                .lock()
                .map_err(|_| formal_error("INTERNAL_ERROR", "Terminal route is unavailable"))?
                .as_ref()
                .is_some_and(|current| current.terminal_id == terminal_id)
            {
                *route_state.lock().map_err(|_| {
                    formal_error("INTERNAL_ERROR", "Terminal route is unavailable")
                })? = None;
            }
            Ok(TerminalDataResponse::Closed { terminal_id })
        }
        TerminalDataRequest::ReadState => {
            let response = gateway.handle(Phase3Request::ReadState, output).map_err(map_error)?;
            match response {
                Phase3Response::State { attached } => Ok(TerminalDataResponse::State {
                    attached: attached
                        .map(|stream| from_phase_stream(&stream, route_state))
                        .transpose()?,
                }),
                _ => {
                    Err(formal_error("INTERNAL_ERROR", "Terminal engine returned an invalid state"))
                }
            }
        }
    }
}

fn attach(
    storage: &Storage,
    gateway: &mut Gateway,
    output: &SharedWriter,
    route_state: &Arc<Mutex<Option<Route>>>,
    terminal_id: &str,
) -> Result<TerminalDataResponse, TerminalDataError> {
    validate_id(terminal_id).map_err(map_core_error)?;
    let terminal_record = storage.terminal(terminal_id).map_err(map_core_error)?;
    if terminal_record.lifecycle_status == "closed" {
        return Err(formal_error("INVALID_STATE", "Terminal is closed"));
    }
    if !terminal::managed_session_exists(storage, &terminal_record.project_id)
        .map_err(map_core_error)?
    {
        return Err(formal_error("NOT_FOUND", "managed tmux session is unavailable"));
    }
    let route = route_for(storage, &terminal_record).map_err(map_core_error)?;
    let response = gateway
        .handle(
            Phase3Request::Attach {
                project_id: route.project_key.clone(),
                window_id: route.window_id.clone(),
            },
            output,
        )
        .map_err(map_error)?;
    *route_state
        .lock()
        .map_err(|_| formal_error("INTERNAL_ERROR", "Terminal route is unavailable"))? =
        Some(route);
    match response {
        Phase3Response::Attached { stream } => {
            Ok(TerminalDataResponse::Attached { stream: from_phase_stream(&stream, route_state)? })
        }
        _ => Err(formal_error("INTERNAL_ERROR", "Terminal engine returned an invalid attachment")),
    }
}

fn route_for(
    storage: &Storage,
    terminal: &muxlane_core::model::Terminal,
) -> Result<Route, CoreError> {
    let project = storage.project(&terminal.project_id)?;
    let project_key = project
        .tmux_session_name
        .strip_prefix(FORMAL_SESSION_PREFIX)
        .ok_or_else(|| CoreError::new("INTERNAL_ERROR", "Project tmux identity is invalid"))?
        .to_owned();
    Ok(Route {
        project_id: terminal.project_id.clone(),
        terminal_id: terminal.terminal_id.clone(),
        project_key,
        window_id: terminal.tmux_window_identity.clone(),
    })
}

fn to_phase_stream(
    stream: &TerminalStream,
    route_state: &Arc<Mutex<Option<Route>>>,
) -> Result<AttachedTerminal, TerminalDataError> {
    let route = route_state
        .lock()
        .map_err(|_| formal_error("INTERNAL_ERROR", "Terminal route is unavailable"))?;
    let route = route
        .as_ref()
        .ok_or_else(|| formal_error("STALE_STREAM", "Terminal stream identity is stale"))?;
    if stream.project_id != route.project_id
        || stream.terminal_id != route.terminal_id
        || stream.window_id != route.window_id
    {
        return Err(formal_error("STALE_STREAM", "Terminal stream identity is stale"));
    }
    Ok(AttachedTerminal {
        connection_id: stream.connection_id.clone(),
        attachment_id: stream.attachment_id,
        bootstrap_id: stream.bootstrap_id,
        project_id: route.project_key.clone(),
        window_id: stream.window_id.clone(),
        pane_id: stream.pane_id.clone(),
    })
}

fn from_phase_stream(
    stream: &AttachedTerminal,
    route_state: &Arc<Mutex<Option<Route>>>,
) -> Result<TerminalStream, TerminalDataError> {
    let route = route_state
        .lock()
        .map_err(|_| formal_error("INTERNAL_ERROR", "Terminal route is unavailable"))?;
    let route = route
        .as_ref()
        .ok_or_else(|| formal_error("STALE_STREAM", "Terminal stream route is unavailable"))?;
    if stream.project_id != route.project_key || stream.window_id != route.window_id {
        return Err(formal_error("STALE_STREAM", "Terminal stream identity is stale"));
    }
    Ok(TerminalStream {
        connection_id: stream.connection_id.clone(),
        attachment_id: stream.attachment_id,
        bootstrap_id: stream.bootstrap_id,
        project_id: route.project_id.clone(),
        terminal_id: route.terminal_id.clone(),
        window_id: stream.window_id.clone(),
        pane_id: stream.pane_id.clone(),
    })
}

fn map_event(event: Phase3Event, route: &Arc<Mutex<Option<Route>>>) -> Option<TerminalDataEvent> {
    let convert = |stream: AttachedTerminal| from_phase_stream(&stream, route).ok();
    match event {
        Phase3Event::History { stream, sequence, bytes } => {
            Some(TerminalDataEvent::History { stream: convert(stream)?, sequence, bytes })
        }
        Phase3Event::Output { stream, sequence, bytes } => {
            Some(TerminalDataEvent::Output { stream: convert(stream)?, sequence, bytes })
        }
        Phase3Event::StreamClosed { stream, sequence } => {
            Some(TerminalDataEvent::StreamClosed { stream: convert(stream)?, sequence })
        }
        Phase3Event::StreamError { stream, sequence, code } => {
            Some(TerminalDataEvent::StreamError { stream: convert(stream)?, sequence, code })
        }
        Phase3Event::ConnectionClosed { .. } => None,
    }
}

fn emit_response(
    output: &SharedWriter,
    id: u64,
    result: Result<TerminalDataResponse, TerminalDataError>,
) -> Result<(), CoreError> {
    let result = match result {
        Ok(response) => TerminalDataResult::Ok { response },
        Err(error) => TerminalDataResult::Error { error },
    };
    let mut output = output
        .lock()
        .map_err(|_| CoreError::new("INTERNAL_ERROR", "Terminal output is unavailable"))?;
    serde_json::to_writer(&mut *output, &TerminalDataFrame::Response { id, result })?;
    output.write_all(b"\n")?;
    output.flush()?;
    Ok(())
}

fn map_error(error: Phase3Error) -> TerminalDataError {
    let code = match error.code.as_str() {
        "validation" => "INVALID_REQUEST",
        "unavailable" => "CAPABILITY_UNAVAILABLE",
        "not_found" => "NOT_FOUND",
        "conflict" => "CONFLICT",
        "state" => "INVALID_STATE",
        "stale_stream" => "STALE_STREAM",
        _ => "INTERNAL_ERROR",
    };
    formal_error(code, &error.message)
}

fn map_core_error(error: CoreError) -> TerminalDataError {
    formal_error(error.code, &error.message)
}
fn core_from_phase3(error: Phase3Error) -> CoreError {
    CoreError::new("CAPABILITY_UNAVAILABLE", error.message)
}
fn formal_error(code: &str, message: &str) -> TerminalDataError {
    TerminalDataError { code: code.to_owned(), message: message.to_owned() }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn formal_protocol_has_no_command_or_target_request() {
        let malicious =
            r#"{"id":1,"request":{"method":"terminal.execute","params":{"command":"id"}}}"#;
        assert!(serde_json::from_str::<TerminalDataRequestEnvelope>(malicious).is_err());
        assert!(
            serde_json::to_string(&TerminalDataRequest::Attach {
                terminal_id: "terminal_safe".to_owned()
            })
            .unwrap()
            .contains("terminal_safe")
        );
    }
}
