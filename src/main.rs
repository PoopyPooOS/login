use ipc_userd::{Error, User, Userd};
use nix::unistd::{setuid, Uid};
use rpassword::read_password;
use std::{
    env,
    io::{stdin, stdout, Write},
    process::Command,
};

fn main() {
    let mut userd = Userd::new("/tmp/init/services/userd.sock");

    let user = login_prompt(&mut userd);

    setuid(Uid::from(user.uid)).expect("Failed to set uid");

    env::set_var("HOME", &user.home);
    env::set_var("USER", &user.username);

    Command::new(user.shell)
        .current_dir(user.home)
        .spawn()
        .expect("Failed to launch shell");
}

fn login_prompt(userd: &mut Userd) -> User {
    loop {
        let username = prompt("Username");

        let user = match userd.fetch_user(username) {
            Ok(user) => user,
            Err(err) => {
                match err {
                    Error::NoSuchUser => eprintln!("No such user"),
                    Error::WrongPassword => {
                        eprintln!("Wrong password!");
                    }
                    Error::UserAlreadyExists => unreachable!("fetch_user can not return this error"),
                }

                continue;
            }
        };

        let password = if user.password.is_some() {
            password_prompt("password")
        } else {
            String::default()
        };

        let result = userd.verify_password(user.uid, password);

        if let Err(err) = result {
            match err {
                Error::NoSuchUser => eprintln!("No such user"),
                Error::WrongPassword => {
                    eprintln!("Wrong password!");
                }
                Error::UserAlreadyExists => unreachable!("fetch_user can not return this error"),
            }
        } else {
            println!();
            break user;
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
