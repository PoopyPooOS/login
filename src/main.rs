use ipc::IpcError;
use ipc_userd::{Error, User, Userd};
use logger::{error, fatal, warn};
use nix::{
    sys::signal::{kill, Signal::SIGUSR1},
    unistd::{setuid, Pid, Uid},
};
use rpassword::read_password;
use std::{
    env,
    io::{stdin, stdout, Write},
    process::{self, Command},
};

fn main() -> Result<(), IpcError> {
    logger::unset_app_name!();
    let serviced_pid = env::var("SERVICED_PID")
        .unwrap_or_else(|_| {
            fatal!("SERVICED_PID environment variable not set, was this launched manually?");
            process::exit(1);
        })
        .parse::<i32>()
        .unwrap_or_else(|_| {
            fatal!("SERVICED_PID environment variable is not an integer");
            process::exit(1);
        });

    let mut userd = Userd::new("/tmp/ipc/services/userd.sock")?;

    match kill(Pid::from_raw(serviced_pid), SIGUSR1) {
        Ok(()) => (),
        Err(err) => {
            warn!(format!("Failed to send ready signal to serviced: {err:#?}"));
        }
    }

    loop {
        let user = login_prompt(&mut userd);

        // Prepare environment
        setuid(Uid::from(user.uid)).expect("Failed to set uid");
        env::set_var("HOME", &user.home);
        env::set_var("USER", &user.username);

        // The shell's exit code is irrelevant
        let _ = Command::new(user.shell)
            .current_dir(user.home)
            .spawn()
            .expect("Failed to launch shell")
            .wait();
    }
}

fn login_prompt(userd: &mut Userd) -> User {
    loop {
        let username = prompt("Username");

        let user = match userd.fetch_user(username) {
            Ok(user) => user,
            Err(err) => {
                match err {
                    Error::NoSuchUser => error!("No such user"),
                    Error::WrongPassword => error!("Wrong password"),
                    Error::UserAlreadyExists => unreachable!("fetch_user can not return this error"),
                    Error::IpcError(ipc_error) => error!(format!("IPC error: {ipc_error}")),
                }

                continue;
            }
        };

        let password = user.password.is_some().then(|| password_prompt("password")).unwrap_or_default();

        match userd.verify_password(user.uid, password) {
            Ok(()) => {
                println!();
                break user;
            }
            Err(err) => match err {
                Error::NoSuchUser => error!("No such user"),
                Error::WrongPassword => error!("Wrong password"),
                Error::UserAlreadyExists => unreachable!("verify_password can not return this error"),
                Error::IpcError(ipc_error) => error!(format!("IPC error: {ipc_error}")),
            },
        }
    }
}

fn prompt(prompt: &str) -> String {
    print!("{prompt}: ");
    stdout().flush().unwrap();

    let mut input = String::new();
    stdin().read_line(&mut input).unwrap();

    input.trim_end_matches('\n').to_string()
}

fn password_prompt(prompt: &str) -> String {
    print!("{prompt}: ");
    stdout().flush().unwrap();

    read_password().unwrap()
}
