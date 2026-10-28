use std::fmt;
use std::io::{self, Write};

/// Exits with status 141 on a broken stdout/stderr pipe; preserves other I/O results.
/// Callers must not pass network or filesystem operation results.
pub fn handle_broken_pipe<T>(result: io::Result<T>) -> io::Result<T> {
    match result {
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => std::process::exit(141),
        result => result,
    }
}

struct Output<W>(W);

impl<W: Write> Write for Output<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        handle_broken_pipe(self.0.write(bytes))
    }

    fn flush(&mut self) -> io::Result<()> {
        handle_broken_pipe(self.0.flush())
    }
}

/// Locks stdout and exits with status 141 if its reader closes.
pub fn stdout() -> impl Write {
    Output(io::stdout().lock())
}

/// Locks stderr and exits with status 141 if its reader closes.
pub fn stderr() -> impl Write {
    Output(io::stderr().lock())
}

fn print_to(mut output: impl Write, args: fmt::Arguments<'_>) {
    output
        .write_fmt(args)
        .and_then(|()| output.flush())
        .unwrap_or_else(|error| panic!("failed writing output: {}", error));
}

/// Prints to stdout, exiting with status 141 on a broken pipe and panicking on other errors.
pub fn print(args: fmt::Arguments<'_>) {
    print_to(stdout(), args);
}

/// Prints a line to stdout with the same error handling as [`print`].
pub fn println(args: fmt::Arguments<'_>) {
    print(format_args!("{}\n", args));
}

/// Prints to stderr, exiting with status 141 on a broken pipe and panicking on other errors.
pub fn eprint(args: fmt::Arguments<'_>) {
    print_to(stderr(), args);
}

/// Prints a line to stderr with the same error handling as [`eprint`].
pub fn eprintln(args: fmt::Arguments<'_>) {
    eprint(format_args!("{}\n", args));
}

/// `std::print!` that exits with status 141 on a broken pipe.
#[macro_export]
macro_rules! print {
    ($($args:tt)*) => {
        $crate::output::print(::core::format_args!($($args)*))
    };
}

/// `std::println!` that exits with status 141 on a broken pipe.
#[macro_export]
macro_rules! println {
    () => {
        $crate::output::print(::core::format_args!("\n"))
    };
    ($($args:tt)*) => {
        $crate::output::println(::core::format_args!($($args)*))
    };
}

/// `std::eprint!` that exits with status 141 on a broken pipe.
#[macro_export]
macro_rules! eprint {
    ($($args:tt)*) => {
        $crate::output::eprint(::core::format_args!($($args)*))
    };
}

/// `std::eprintln!` that exits with status 141 on a broken pipe.
#[macro_export]
macro_rules! eprintln {
    () => {
        $crate::output::eprint(::core::format_args!("\n"))
    };
    ($($args:tt)*) => {
        $crate::output::eprintln(::core::format_args!($($args)*))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Denied;

    impl Write for Denied {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::PermissionDenied.into())
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::ErrorKind::PermissionDenied.into())
        }
    }

    #[test]
    fn preserves_non_pipe_results() {
        assert_eq!(handle_broken_pipe(Ok(3)).unwrap(), 3);
        let mut output = Output(Denied);
        assert_eq!(
            output.write(b"value").unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        assert_eq!(
            output.flush().unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
    }

    #[test]
    fn formatting_preserves_bytes() {
        let mut bytes = Vec::new();
        let value = 42;
        print_to(&mut bytes, format_args!("{value:04}\n"));
        assert_eq!(bytes, b"0042\n");
    }

    #[test]
    #[should_panic(expected = "failed writing output")]
    fn printing_preserves_other_errors() {
        print_to(Output(Denied), format_args!("value"));
    }
}
