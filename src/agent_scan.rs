// SPDX-License-Identifier: Apache-2.0
//! The editor's side of agent detection (PLAN.md, "Detecting and connecting
//! to agents", and "Agent experience", sections 1 and 2): the `:AgentScan`
//! command, the config's opt-in, and the state the status indicator shows.
//!
//! The editor never scans. It asks for one through [`AgentCmd::Scan`]; the
//! front end runs `agent_detect::scan` on a worker thread and the result
//! comes back as `Event::AgentScan`. Nothing here connects to an agent.

use crate::Editor;
use crate::agent_detect::ScanSummary;

/// Work the editor asks the front end to do for the agent layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentCmd {
    /// Run a scan on a worker and send its summary as `Event::AgentScan`.
    Scan,
}

impl Editor {
    /// `:AgentScan`. Turns the agent layer on for this session, so the status
    /// indicator shows, and asks for a scan.
    pub fn agent_scan_command(&mut self) {
        self.agent_active = true;
        self.agent_outbox.push(AgentCmd::Scan);
        self.info("Scanning for agents...");
    }

    /// The config's opt-in (`[agent] enabled = true` or `--agent`). It sets
    /// the opt-in flag that gates the conflict prompt and the write guard
    /// (`conflict.rs`). When on, the layer is active and a scan runs at
    /// startup. When off, nothing happens.
    pub(crate) fn agent_opt_in(&mut self, enabled: bool) {
        self.set_agent_enabled(enabled);
        if enabled {
            self.agent_active = true;
            self.agent_outbox.push(AgentCmd::Scan);
        }
    }

    /// The requests not yet taken. `main.rs` sends each to its worker.
    pub fn take_agent_commands(&mut self) -> Vec<AgentCmd> {
        std::mem::take(&mut self.agent_outbox)
    }

    /// A scan finished: keeps the result for the indicator and shows it on
    /// the message line.
    pub(crate) fn agent_arrived(&mut self, scan: ScanSummary) {
        self.info(scan.message());
        self.agent_scan = Some(scan);
    }
}
