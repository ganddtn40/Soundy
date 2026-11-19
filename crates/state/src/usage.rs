//! Install reporting. Upstream Soundy asks whether it may send an anonymous install ping to
//! Soundy's own server. Soundy has no reporting endpoint of its own and must not send a fork's
//! users to the upstream one, so this build keeps the interface the login screen expects and
//! reports nothing.

use gpui::{Context, Entity};

use crate::{Io, Session};

pub struct Usage;

impl Usage {
    pub fn new(
        _session: Entity<Session>,
        _database: storage::Database,
        _io: Io,
        _cx: &mut Context<Self>,
    ) -> Self {
        Self
    }

    /// Whether the login screen should ask for consent. Soundy has nothing to ask about.
    pub fn asking(&self) -> bool {
        false
    }

    pub fn consented(&self) -> bool {
        false
    }

    pub fn consent(&mut self, _consented: bool, _cx: &mut Context<Self>) {}

    pub fn report(&mut self, _cx: &mut Context<Self>) {}
}
