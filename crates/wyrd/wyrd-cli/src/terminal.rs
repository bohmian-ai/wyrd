//! Interruptible line input from the invoking terminal, shared by local
//! startup questions and the MCP host offer, and the terminal handed to the
//! coding agent guided setup starts.

use std::io::{self, Write};
use std::os::fd::OwnedFd;

use rustix::fs::{Mode, OFlags};
use rustix::termios::{LocalModes, OptionalActions, Termios, tcgetattr, tcsetattr};
use secrecy::SecretString;
use tokio::io::unix::AsyncFd;

/// The invoking terminal opened nonblocking, so an awaiting question stays
/// cancellable by interruption or a supervised child's exit.
pub(crate) struct Terminal {
    /// The actual tty device: macOS kqueue cannot watch the `/dev/tty` alias.
    device: AsyncFd<OwnedFd>,
}

/// Restores the terminal's saved attributes when concealed input ends,
/// including when the awaiting question is cancelled.
struct EchoGuard<'a> {
    /// Terminal whose echo was disabled.
    device: &'a OwnedFd,
    /// Attributes in effect before concealment.
    saved: Termios,
}

impl Drop for EchoGuard<'_> {
    /// Re-enable echo; a failure leaves nothing further to restore.
    fn drop(&mut self) {
        let _ = tcsetattr(self.device, OptionalActions::Now, &self.saved);
    }
}

impl Terminal {
    /// Open the terminal attached to standard input.
    ///
    /// # Errors
    /// Propagates terminal name, open, and reactor-registration failures.
    pub(crate) fn open() -> io::Result<Self> {
        let path = rustix::termios::ttyname(io::stdin(), Vec::new())?;
        Ok(Self {
            device: AsyncFd::new(rustix::fs::open(
                path,
                OFlags::RDONLY | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            )?)?,
        })
    }

    /// Print `question` and read one answer line, without its line ending.
    ///
    /// # Errors
    /// Propagates output, input, and UTF-8 failures, and
    /// [`io::ErrorKind::UnexpectedEof`] when input ends before an answer.
    pub(crate) async fn ask(&self, question: &str) -> io::Result<String> {
        let mut output = io::stdout();
        write!(output, "{question}")?;
        output.flush()?;
        let mut answer = Vec::new();
        loop {
            let mut ready = self.device.readable().await?;
            let mut byte = [0];
            let Ok(read) = ready
                .try_io(|fd| rustix::io::read(fd.get_ref(), &mut byte).map_err(io::Error::from))
            else {
                continue;
            };
            if read? == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "no answer was entered",
                ));
            }
            if byte[0] == b'\n' {
                break;
            }
            answer.push(byte[0]);
        }
        let answer = String::from_utf8(answer)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        Ok(answer.trim_end_matches('\r').to_owned())
    }

    /// Ask `question` with echo disabled for the answer, which is returned
    /// only as a secret and never displayed.
    ///
    /// # Errors
    /// Propagates terminal-attribute failures and those of [`Terminal::ask`].
    pub(crate) async fn conceal(&self, question: &str) -> io::Result<SecretString> {
        let device = self.device.get_ref();
        let saved = tcgetattr(device)?;
        let mut quiet = saved.clone();
        quiet.local_modes.remove(LocalModes::ECHO);
        tcsetattr(device, OptionalActions::Now, &quiet)?;
        let _restore = EchoGuard { device, saved };
        let answer = self.ask(question).await.map(SecretString::from);
        println!();
        answer
    }
}

/// The invoking terminal's attributes saved before a child takes it over,
/// restored once that child has been reaped so the terminal stays usable
/// even if the child left it in raw mode.
pub(crate) struct SavedTerminal(
    /// Standard input's attributes as read by [`SavedTerminal::save`] before
    /// the child started; [`SavedTerminal::restore`] writes them back.
    Termios,
);

impl SavedTerminal {
    /// Save standard input's terminal attributes.
    ///
    /// # Errors
    /// Propagates the attribute read failure, such as standard input not
    /// being a terminal.
    pub(crate) fn save() -> io::Result<Self> {
        Ok(Self(tcgetattr(io::stdin())?))
    }

    /// Restore the saved attributes; a failure leaves nothing further to
    /// restore.
    pub(crate) fn restore(&self) {
        let _ = tcsetattr(io::stdin(), OptionalActions::Now, &self.0);
    }
}
