pub fn print_table(table: &impl prettytable::AsTableSlice) -> std::io::Result<usize> {
    rpc_toolkit::output::handle_broken_pipe(table.as_slice().print_tty(false))
}
