use ipc_userd::{Error, User, Userd};
use logger::error;
use nix::{
    fcntl::{OFlag, open},
    libc::login_tty,
    sys::stat::Mode,
    unistd::{Uid, setsid, setuid},
};
use rpassword::read_password;
use std::{
    env,
    fmt::Display,
    io::{Write, stdin, stdout},
    os::fd::AsRawFd,
    process::Command,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    logger::unset_app_name!();
    logger::panic::set_panic_hook();

    let serviced_pid = ipc_serviced::get_pid();

    let mut userd = Userd::new("/tmp/ipc/services/userd.sock")?;

    ipc_serviced::ready(serviced_pid);

    loop {
        let user = login_prompt(&mut userd);

        let tty_fd = open("/dev/tty0", OFlag::O_RDWR, Mode::empty())?;

        setsid()?;

        unsafe { login_tty(tty_fd.as_raw_fd()) };

        // Drop privileges to the logged-in user
        setuid(Uid::from(user.uid)).expect("Failed to set UID");

        unsafe {
            env::set_var("HOME", &user.home);
            env::set_var("USER", &user.username);
        }

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
                    Error::UserAlreadyExists => {
                        unreachable!("fetch_user can not return this error")
                    }
                    Error::IpcError(ipc_error) => error!("IPC error: {ipc_error}"),
                }

                continue;
            }
        };

        let password = user
            .password
            .is_some()
            .then(|| password_prompt("password"))
            .unwrap_or_default();

        match userd.verify_password(user.uid, password) {
            Ok(()) => {
                println!();
                break user;
            }
            Err(err) => match err {
                Error::NoSuchUser => error!("No such user"),
                Error::WrongPassword => error!("Wrong password"),
                Error::UserAlreadyExists => {
                    unreachable!("verify_password can not return this error")
                }
                Error::IpcError(ipc_error) => error!("IPC error: {ipc_error}"),
            },
        }
    }
}

fn prompt(prompt: impl Display) -> String {
    print!("{prompt}: ");
    stdout().flush().unwrap();

    let mut input = String::new();
    stdin().read_line(&mut input).unwrap();

    input.trim_end_matches('\n').to_string()
}

fn password_prompt(prompt: impl Display) -> String {
    print!("{prompt}: ");
    stdout().flush().unwrap();

    read_password().unwrap()
}
