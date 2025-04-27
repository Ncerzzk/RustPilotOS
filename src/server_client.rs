use sendfd::{RecvWithFd, SendWithFd};
use std::{
    cell::Cell, ffi::CStr, io::{self, BufRead, BufReader, Read, Write}, mem::MaybeUninit, os::{
        fd::{AsRawFd, FromRawFd},
        unix::net::{UnixListener, UnixStream},
    }, path::Path
};

use crate::module::Module;

thread_local! {
   static CLIENT_STDIN:Cell<libc::c_int> = Cell::new(-1);
   static CLIENT_STDOUT:Cell<libc::c_int> = Cell::new(-1); 
}

pub fn server_init<P: AsRef<Path>>(socket_path: P) -> Result<(), std::io::Error> {
    let _ = std::fs::remove_file(socket_path.as_ref());

    let listener = UnixListener::bind(socket_path.as_ref()).unwrap();
    listener.set_nonblocking(true).unwrap();

    loop {
        if let Ok((mut client, _)) = listener.accept() {
            let mut fds: [libc::c_int; 2] = [0; 2];
            let mut buf: [u8; 10] = [0; 10];
            unsafe {
                client
                    .recv_with_fd(
                        &mut buf,
                        std::slice::from_raw_parts_mut(fds.as_mut_ptr(), 2),
                    )
                    .unwrap();
            }

            let mut buffer = [0; 100];
            client.read(&mut buffer).unwrap();

            let cmd_raw = CStr::from_bytes_until_nul(&buffer)
                .unwrap()
                .to_str()
                .unwrap()
                .to_string();
            if cmd_raw.contains("shutdown") {
                break;
            }

            let client_cp = client.try_clone().unwrap();
            let _x = std::thread::spawn(move ||{
                let cmd_with_args: Vec<_> = cmd_raw.split_whitespace().collect();
                assert!(cmd_with_args.len() >= 1);

                CLIENT_STDIN.set( fds[0]);
                CLIENT_STDOUT.set(fds[1]);

                Module::get_module(cmd_with_args[0])
                    .execute((cmd_with_args.len()) as u32, cmd_with_args.as_ptr());
                _ = client_cp.shutdown(std::net::Shutdown::Both);
            });
        }
    }

    Ok(())
}

pub struct Client {
    stream: UnixStream,
}

impl Client {
    pub fn new<P: AsRef<Path>>(socket_path: P) -> Result<Client, io::Error> {
        let stream = UnixStream::connect(socket_path.as_ref())?;
        let mut client = Client { stream };
        client.send_stdin_out();
        Ok(client)
    }

    pub fn block_read(&mut self) {
        let mut bufreader = BufReader::new(self.stream.try_clone().unwrap());
        let mut str_out: String = String::new();
        while let Ok(n) = bufreader.read_line(&mut str_out) {
            if n == 0 {
                break;
            }
            print!("{}", str_out);
            str_out.clear();
        }
    }

    pub fn send_str(&mut self, data: &str) {
        self.stream.write_all(data.as_bytes()).unwrap();
        self.stream.flush().unwrap();
    }

    fn send_stdin_out(&mut self) {
        let pipe: [libc::c_int; 2] = [std::io::stdin().as_raw_fd(), std::io::stdout().as_raw_fd()];
        unsafe {
            self.stream
                .send_with_fd(&[15], std::slice::from_raw_parts(pipe.as_ptr(), 2))
                .unwrap();
        }
    }
}

#[macro_export]
macro_rules! thread_logln {
    ($($arg:tt)*) => {
        write!(rpos::server_client::get_output(),"{}\n", format!($($arg)*)).unwrap()
    }
}

#[macro_export]
macro_rules! thread_log {
    ($($arg:tt)*) => {
        write!(rpos::server_client::get_output(),"{}", format!($($arg)*)).unwrap()
    }
}

pub fn get_output() -> Box<dyn Write> {
    let output = CLIENT_STDOUT.get();
    if output == -1 {
        Box::new(std::io::stdout()) as Box<dyn Write>
    } else {
        // let stream: &ThreadSpecificData = unsafe { &mut *(thread_data as *mut ThreadSpecificData) };
        // unsafe { Box::new((*stream.stream).try_clone().unwrap()) as Box<dyn Write> }
        let fd = unsafe { std::fs::File::from_raw_fd(CLIENT_STDOUT.get()) };
        Box::new(fd) as Box<dyn Write>
    }
}

pub fn setup_client_stdin_out() -> Result<(), ()> {
    if CLIENT_STDIN.get() == -1 || CLIENT_STDOUT.get() == -1{
        return Err(());
    }

    unsafe {
        let mut s: MaybeUninit<libc::termios> = MaybeUninit::zeroed();
        libc::tcgetattr(CLIENT_STDIN.get(), s.as_mut_ptr());
        let mut term = s.assume_init();
        term.c_lflag &= !libc::ICANON;
        term.c_lflag &= !libc::ECHO;
        libc::tcsetattr(
            CLIENT_STDIN.get(),
            libc::TCSANOW,
            &term as *const libc::termios,
        );

        libc::dup2(CLIENT_STDIN.get(), libc::STDIN_FILENO);
        libc::dup2(CLIENT_STDOUT.get(), libc::STDOUT_FILENO);
    }
    Ok(())
}
