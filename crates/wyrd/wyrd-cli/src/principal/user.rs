//! `wyrd principal user create|invite`: an administrator adds a local
//! password User directly, or issues a one-time signup invitation. Each verb
//! is one call through `wyrd_client::Principals`.

use std::io;
use std::process::ExitCode;

use clap::{Args, Subcommand};
use wyrd_spec::auth::{CreateUserInvitationRequest, CreateUserRequest, SecretBearer};

use crate::error::WyrdCliError;
use crate::principal::credential::TenantEndpoint;

/// Local User verbs; both need User management and tenant administration.
#[derive(Debug, Subcommand)]
pub enum UserCommand {
    /// Create a local User; the password is read from standard input.
    Create(CreateUserArgs),
    /// Issue a one-time signup invitation and print its code, once.
    Invite(InviteArgs),
}

/// The User to create.
#[derive(Debug, Args)]
pub struct CreateUserArgs {
    /// Username the User signs in with.
    #[arg(long, value_name = "NAME")]
    pub username: String,
    /// Role to grant, such as `admin`; repeat for several.
    #[arg(long = "role", value_name = "ROLE", required = true)]
    pub roles: Vec<String>,
    /// Deployment; the credential comes from the ambient chain.
    #[command(flatten)]
    pub endpoint: TenantEndpoint,
}

/// The invitation to issue.
#[derive(Debug, Args)]
pub struct InviteArgs {
    /// Role the enrolled User receives, such as `editor`; repeat for several.
    #[arg(long = "role", value_name = "ROLE", required = true)]
    pub roles: Vec<String>,
    /// Deployment; the credential comes from the ambient chain.
    #[command(flatten)]
    pub endpoint: TenantEndpoint,
}

/// Dispatch one local User verb.
///
/// # Errors
/// Returns [`WyrdCliError::Io`] when standard input cannot be read, and the
/// server's stable Wyrd error when the caller is unauthorized, the username
/// or password is outside policy or taken, or a Role does not exist.
pub async fn dispatch(command: UserCommand) -> Result<ExitCode, WyrdCliError> {
    match command {
        UserCommand::Create(args) => create(args).await,
        UserCommand::Invite(args) => invite(args).await,
    }
}

/// Create the User with the password on standard input, minus one trailing
/// line ending, so it never enters argv or shell history.
///
/// # Errors
/// Returns the errors documented on [`dispatch`].
async fn create(args: CreateUserArgs) -> Result<ExitCode, WyrdCliError> {
    let mut input = String::new();
    io::Read::read_to_string(&mut io::stdin().lock(), &mut input)
        .map_err(|source| WyrdCliError::Io { source })?;
    let password = input.strip_suffix('\n').map_or(input.as_str(), |line| {
        line.strip_suffix('\r').unwrap_or(line)
    });
    let created = crate::client::principals(args.endpoint.server.as_str())
        .await?
        .create_user(&CreateUserRequest {
            username: args.username,
            password: SecretBearer::new(password.to_owned()),
            roles: args.roles,
        })
        .await
        .map_err(|source| WyrdCliError::Server { source })?;
    println!("principal_id: {}", created.principal_id);
    println!("username: {}", created.username);
    Ok(ExitCode::SUCCESS)
}

/// Issue the invitation and print its code, which is shown only once.
///
/// # Errors
/// Returns the errors documented on [`dispatch`].
async fn invite(args: InviteArgs) -> Result<ExitCode, WyrdCliError> {
    let issued = crate::client::principals(args.endpoint.server.as_str())
        .await?
        .invite_user(&CreateUserInvitationRequest { roles: args.roles })
        .await
        .map_err(|source| WyrdCliError::Server { source })?;
    println!("invitation_id: {}", issued.invitation.id);
    println!("invitation_code: {}", issued.invitation_code.expose());
    println!("signup_url: {}", issued.signup_url);
    println!("expires_at: {}", issued.invitation.expires_at.to_rfc3339());
    println!("This is the only time the invitation code is shown. Deliver it separately.");
    Ok(ExitCode::SUCCESS)
}
