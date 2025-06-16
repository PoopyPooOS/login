use error::Error;
use logger::error;
use nix::{
    fcntl::{open, OFlag},
    libc::login_tty,
    sys::stat::Mode,
    unistd::{setsid, setuid, Uid},
};
use prelude::logger;
use rpassword::read_password;
use std::{
    env,
    fmt::Display,
    io::{stdin, stdout, Write},
    os::fd::AsRawFd,
    process::Command,
};
use userd::{User, Userd};

mod error {
    #[prelude::error_enum]
    pub enum Error {
        #[error("Failed to signal service daemon: {0}")]
        Serviced(#[from] serviced::Error),
        #[error("Userd error: {0}")]
        Userd(#[from] userd::Error),
        #[error("IPC Error: {0}")]
        IpcError(#[from] ipc::error::IpcError),
        #[error("Nix Error: {0}")]
        Nix(#[from] nix::Error),
    }
}

#[prelude::entry(err: Error)]
fn main() {
    logger::unset_app_name!();
    logger::panic::set_panic_hook();

    let serviced_pid = serviced::get_pid()?;

    let mut userd = Userd::new("/tmp/ipc/services/userd.sock")?;

    serviced::ready(serviced_pid)?;

    loop {
        let user = login_prompt(&mut userd)?;

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

fn login_prompt(userd: &mut Userd) -> Result<User, Error> {
    loop {
        let username = prompt("Username");

        let user = match userd.fetch_user(username) {
            Ok(user) => user,
            Err(err) => {
                error!("{err}");
                continue;
            }
        };

        let password = if user.password.is_some() {
            password_prompt("password")
        } else {
            String::default()
        };

        if let Err(err) = userd.verify_password(user.uid, password) {
            error!("{err}");
        }

        println!();
        break Ok(user);
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
