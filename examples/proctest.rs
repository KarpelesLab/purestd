#![no_std]
#![no_main]
use purestd::prelude::*;
use purestd::process::{Command, Stdio};
use core::str::from_utf8;

fn main() {
    // output(): capture stdout
    let out = Command::new("echo").arg("hello").arg("world").output().unwrap();
    println!("echo status.success={} stdout={:?}", out.status.success(), from_utf8(&out.stdout).unwrap().trim_end());

    // status(): exit code via `sh -c 'exit 3'`
    let st = Command::new("sh").arg("-c").arg("exit 3").status().unwrap();
    println!("sh exit code = {:?}", st.code());

    // env: pass a var, read it back via printenv
    let out = Command::new("sh").arg("-c").arg("echo $PURESTD_PROC")
        .env("PURESTD_PROC", "xyz").output().unwrap();
    println!("env passthrough = {:?}", from_utf8(&out.stdout).unwrap().trim_end());

    // PATH search: `true` resolves via PATH and succeeds
    let st = Command::new("true").status().unwrap();
    println!("true success = {}", st.success());

    // not found
    let r = Command::new("definitely_not_a_real_program_xyz").status();
    println!("missing program -> err kind = {:?}", r.err().map(|e| e.kind()));

    // pipe to stdin via sh wc -c
    let mut child = Command::new("cat").stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    {
        use purestd::io::Write;
        child.stdin.take().unwrap().write_all(b"piped-data").unwrap();
    }
    let out = child.wait_with_output().unwrap();
    println!("cat stdin->stdout = {:?}", from_utf8(&out.stdout).unwrap());

    println!("proctest: OK");
}
purestd::entry!(main);
