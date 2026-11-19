use std::path::PathBuf;

use clap_complete::Shell;
use serde::{Deserialize, Serialize};

/// Type of possible CLI commands: whether is can be executed right there or needs to be sent as request
pub enum CommandType {
    /// Plain `soundy` or `soundy <paths|links>`: claims the instance or hands the items over.
    Launch(Vec<String>),
    /// Answered by this process alone, whether or not Soundy runs.
    Local(Local),
    /// Sent to the running instance, which is started first if the request allows it. The
    /// answer is printed in `Format`.
    Remote(Request, Format),
}

/// How a command line prints what it is answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// For a person to read: words, and tables with a header.
    Text,
    /// For a script: the answer as JSON on one line. Stands for `--json`
    Json,
}

/// Local CLI commands that can be executed without `Soundy` running or requesting is's socket
pub enum Local {
    /// Prints the completion script for a shell. Stands for `soundy completions <shell>`
    Completions(Shell),
}

/// CLI commands that need to sent request to the running `Soundy` application
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Request {
    /// Commands serving playback manipulation
    Playback(PlaybackCommand),
    /// Commands serving queue manipulation
    Queue(QueueCommand),
    /// Commands serving provider manipulation
    Provider(ProviderCommand),
    /// A search on the current provider. Stands for `soundy search <query>`
    Search(SearchQuery),
    /// What plays, sent again every time it changes, until the command line hangs up. Stands
    /// for `soundy watch`
    Watch(Watch),
    /// Commands serving Soundy Session, listening together with other Soundy apps
    Session(SessionCommand),
    /// The lyrics of the current track, as already held, without looking any up. Stands for
    /// `soundy lyrics`
    Lyrics,
    /// Reads or changes whether the current track is a favorite. Stands for `soundy like`
    Like(Option<Switch>),
}

impl Request {
    /// Whether the answer is a stream of replies rather than one, so the command line waits
    /// for as long as it takes rather than giving up on a slow answer.
    pub fn streams(&self) -> bool {
        matches!(self, Self::Watch(_))
    }
}

/// How `soundy watch` follows what plays.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Watch {
    /// Also a line every second while a track plays, for a clock. Stands for `--position`
    pub position: bool,
}

/// What to search for on the current provider, and how much of it to answer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchQuery {
    pub text: String,
    /// Only hits of this kind, every kind when `None`.
    pub kind: Option<Kind>,
    pub limit: u16,
}

/// What a search hit is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Song,
    Album,
    Artist,
    Playlist,
}

/// Commands serving playback manipulation
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlaybackCommand {
    /// Triggers current playback's Play. Stands for `soundy play`
    Play,
    /// Pauses current playback. Stands for `soundy pause`
    Pause,
    /// Pauses what plays, or resumes what is paused. Stands for `soundy toggle`
    Toggle,
    /// Plays next song in the queue. Stands for `soundy next`
    Next,
    /// Plays previous song in the queue. Stands for `soundy prev`
    Previous,
    /// Puts current playback's slider on provided procentage (0% is start, 100% is end of the track). Stands for `soundy seek N`, where N is in [0, 100]
    Seek(u8),
    /// Moves the current track to a time or by a step from where it is, in whole seconds and
    /// held inside the track. Stands for `soundy seek 1:30`, `soundy seek +10` or `soundy seek -10`
    SeekTime(Time),
    /// Returns current track's information. Stands for `soundy current`
    CurrentTrack,
    /// Returns or sets volume in order to `Switch`. Stands for `soundy volume < +/-N|N >`
    Volume(Option<Level>),
    /// Returns or sets playback mute. Stands for `soundy mute [on|off|toggle]`
    Mute(Option<Switch>),
    /// Returns or sets playback shuffle in order to `Switch`. Stands for `soundy shuffle <on|off|toggle>`
    Shuffle(Option<Switch>),
    /// Returns or sets playback repeat in order to `Repeat`. Stands for `soundy repeat <off|all|one>`
    Repeat(Option<Repeat>),
}

/// Commands serving provider manipulation
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderCommand {
    /// Lists every provider, whether it has an account and which one is current. Stands for
    /// `soundy providers`
    List,
    /// Returns the current provider's slug, or switches to the one named, which has to have an
    /// account. Stands for `soundy provider [slug]`
    Current(Option<String>),
}

/// Commands serving Soundy Session
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionCommand {
    /// Returns the session Soundy is in, if any. Stands for `soundy session`
    Status,
    /// Starts a session on the current provider. Stands for `soundy session start`
    Start,
    /// Joins the session a six-digit code names, or opens the dialog that takes one when none is
    /// given. Stands for `soundy session join [code]`
    Join(Option<String>),
    /// Leaves the session, ending it when Soundy hosts it. Stands for `soundy session leave`
    Leave,
}

/// Commands serving queue manipulation
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum QueueCommand {
    /// Returns a read-only snapshot of the queue, without changing playback. Stands for
    /// `soundy queue`
    Snapshot,
    /// Queues items after the ones queued by hand. Stands for `soundy add <items>`
    Add(Vec<Item>),
    /// Queues items right after the current track. Stands for `soundy play-next <items>`
    PlayNext(Vec<Item>),
    /// Plays items at once, right after the current track so the rest of the queue follows
    /// them. Stands for `soundy play <items>`
    Play(Vec<Item>),
}

/// Something to queue: a link of the current provider, or a file the running Soundy reads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Item {
    Link(String),
    /// Always absolute, since the running Soundy sits in another folder.
    Path(PathBuf),
    /// The best hit of this kind for a search on the current provider, the one `soundy search`
    /// lists first. It is queued alone. Stands for `soundy add -s <query>`
    Search {
        text: String,
        kind: Kind,
    },
    /// The best match of this kind in the local library, ranked as the search page ranks it,
    /// with no provider asked. It is queued alone. Stands for `soundy add -s <query> --local`
    Local {
        text: String,
        kind: Kind,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Level {
    To(u8),
    By(i8),
}

/// A point in a track for `seek` to move to: whole seconds from its start, or a signed step
/// from where it plays now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Time {
    To(u64),
    By(i64),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Switch {
    On,
    Off,
    Toggle,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Repeat {
    Off,
    All,
    One,
}
