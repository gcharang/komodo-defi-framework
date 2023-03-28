use std::cell::RefMut;
use std::fmt;
use std::io::Write;
use std::ops::DerefMut;

mod macros {
    #[macro_export]
    macro_rules! write_safe_io {
        ($dst:expr, $($arg:tt)*) => {
            $dst.write_safe(format_args!($($arg)*))
        }
    }
    #[macro_export]
    macro_rules! writeln_safe_io {
        ($dst:expr, $($arg:tt)*) => {{
            write_safe_io!($dst, $($arg)*);
            write_safe_io!($dst, "\n");
        }};
    }
    pub use write_safe_io;
    pub use writeln_safe_io;
}

pub use macros::{write_safe_io, writeln_safe_io};

pub trait WriteSafeIO {
    fn write_safe<'a>(&mut self, args: fmt::Arguments<'_>)
    where
        Self: DerefMut<Target = &'a mut dyn Write>,
    {
        let writer = self.deref_mut();
        Write::write_fmt(writer, args).expect("`write_fmt` should never fail for `WriteSafeIO` types")
    }
}

impl WriteSafeIO for RefMut<'_, &'_ mut dyn Write> {}
