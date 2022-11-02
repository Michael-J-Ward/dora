use eyre::{bail, Context};
use std::{
    env::consts::{DLL_PREFIX, DLL_SUFFIX, EXE_SUFFIX},
    ffi::{OsStr, OsString},
    path::Path,
};

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let target = root.join("target");
    std::env::set_current_dir(root.join(file!()).parent().unwrap())
        .wrap_err("failed to set working dir")?;

    tokio::fs::create_dir_all("build").await?;
    let build_dir = Path::new("build");

    build_package("dora-operator-api-cxx").await?;
    let operator_cxxbridge = target
        .join("cxxbridge")
        .join("dora-operator-api-cxx")
        .join("src");
    tokio::fs::copy(
        operator_cxxbridge.join("lib.rs.cc"),
        build_dir.join("operator-bridge.cc"),
    )
    .await?;
    tokio::fs::copy(
        operator_cxxbridge.join("lib.rs.h"),
        build_dir.join("dora-operator-api.h"),
    )
    .await?;

    build_package("dora-node-api-cxx").await?;
    let node_cxxbridge = target
        .join("cxxbridge")
        .join("dora-node-api-cxx")
        .join("src");
    tokio::fs::copy(
        node_cxxbridge.join("lib.rs.cc"),
        build_dir.join("node-bridge.cc"),
    )
    .await?;
    tokio::fs::copy(
        node_cxxbridge.join("lib.rs.h"),
        build_dir.join("dora-node-api.h"),
    )
    .await?;
    tokio::fs::write(
        build_dir.join("operator.h"),
        r###"#include "../operator-rust-api/operator.h""###,
    )
    .await?;

    build_package("dora-node-api-c").await?;
    build_package("dora-operator-api-c").await?;
    build_cxx_node(
        root,
        &[
            &dunce::canonicalize(Path::new("node-rust-api").join("main.cc"))?,
            &dunce::canonicalize(build_dir.join("node-bridge.cc"))?,
        ],
        "node_rust_api",
        &["-l", "dora_node_api_cxx"],
    )
    .await?;
    build_cxx_node(
        root,
        &[&dunce::canonicalize(
            Path::new("node-c-api").join("main.cc"),
        )?],
        "node_c_api",
        &["-l", "dora_node_api_c"],
    )
    .await?;
    build_cxx_operator(
        &[
            &dunce::canonicalize(Path::new("operator-rust-api").join("operator.cc"))?,
            &dunce::canonicalize(build_dir.join("operator-bridge.cc"))?,
        ],
        "operator_rust_api",
        &[
            "-l",
            "dora_operator_api_cxx",
            "-L",
            &root.join("target").join("debug").to_str().unwrap(),
        ],
    )
    .await?;
    build_cxx_operator(
        &[&dunce::canonicalize(
            Path::new("operator-c-api").join("operator.cc"),
        )?],
        "operator_c_api",
        &[],
    )
    .await?;

    build_package("dora-runtime").await?;

    dora_coordinator::run(dora_coordinator::Args {
        run_dataflow: Path::new("dataflow.yml").to_owned().into(),
        runtime: Some(root.join("target").join("debug").join("dora-runtime")),
    })
    .await?;

    Ok(())
}

async fn build_package(package: &str) -> eyre::Result<()> {
    let cargo = std::env::var("CARGO").unwrap();
    let mut cmd = tokio::process::Command::new(&cargo);
    cmd.arg("build");
    cmd.arg("--package").arg(package);
    if !cmd.status().await?.success() {
        bail!("failed to build {package}");
    };
    Ok(())
}

async fn build_cxx_node(
    root: &Path,
    paths: &[&Path],
    out_name: &str,
    args: &[&str],
) -> eyre::Result<()> {
    let mut clang = tokio::process::Command::new("clang++");
    clang.args(paths);
    clang.arg("-std=c++17");
    #[cfg(target_os = "linux")]
    {
        clang.arg("-l").arg("m");
        clang.arg("-l").arg("rt");
        clang.arg("-l").arg("dl");
        clang.arg("-pthread");
    }
    #[cfg(target_os = "windows")]
    {
        clang.arg("-ladvapi32");
        clang.arg("-luserenv");
        clang.arg("-lkernel32");
        clang.arg("-lws2_32");
        clang.arg("-lbcrypt");
        clang.arg("-lncrypt");
        clang.arg("-lschannel");
        clang.arg("-lntdll");
        clang.arg("-liphlpapi");

        clang.arg("-lcfgmgr32");
        clang.arg("-lcredui");
        clang.arg("-lcrypt32");
        clang.arg("-lcryptnet");
        clang.arg("-lfwpuclnt");
        clang.arg("-lgdi32");
        clang.arg("-lmsimg32");
        clang.arg("-lmswsock");
        clang.arg("-lole32");
        clang.arg("-lopengl32");
        clang.arg("-lsecur32");
        clang.arg("-lshell32");
        clang.arg("-lsynchronization");
        clang.arg("-luser32");
        clang.arg("-lwinspool");

        clang.arg("-Wl,-nodefaultlib:libcmt");
        clang.arg("-D_DLL");
        clang.arg("-lmsvcrt");
    }
    #[cfg(target_os = "macos")]
    {
        clang.arg("-framework").arg("CoreServices");
        clang.arg("-framework").arg("Security");
        clang.arg("-l").arg("System");
        clang.arg("-l").arg("resolv");
        clang.arg("-l").arg("pthread");
        clang.arg("-l").arg("c");
        clang.arg("-l").arg("m");
    }
    clang.args(args);
    clang.arg("-L").arg(root.join("target").join("debug"));
    clang
        .arg("--output")
        .arg(Path::new("../build").join(format!("{out_name}{EXE_SUFFIX}")));
    if let Some(parent) = paths[0].parent() {
        clang.current_dir(parent);
    }

    if !clang.status().await?.success() {
        bail!("failed to compile c++ node");
    };
    Ok(())
}

async fn build_cxx_operator(
    paths: &[&Path],
    out_name: &str,
    link_args: &[&str],
) -> eyre::Result<()> {
    let mut object_file_paths = Vec::new();

    for path in paths {
        let mut compile = tokio::process::Command::new("clang++");
        compile.arg("-c").arg(path);
        compile.arg("-std=c++17");
        let object_file_path = path.with_extension("o");
        compile.arg("-o").arg(&object_file_path);
        #[cfg(unix)]
        compile.arg("-fPIC");
        if let Some(parent) = path.parent() {
            compile.current_dir(parent);
        }
        if !compile.status().await?.success() {
            bail!("failed to compile cxx operator");
        };
        object_file_paths.push(object_file_path);
    }

    let mut link = tokio::process::Command::new("clang++");
    link.arg("-shared").args(&object_file_paths);
    link.args(link_args);
    link.arg("-o")
        .arg(Path::new("../build").join(library_filename(out_name)));
    if let Some(parent) = paths[0].parent() {
        link.current_dir(parent);
    }
    if !link.status().await?.success() {
        bail!("failed to create shared library from cxx operator (c api)");
    };

    Ok(())
}

// taken from `rust_libloading` crate by Simonas Kazlauskas, licensed under the ISC license (
// see https://github.com/nagisa/rust_libloading/blob/master/LICENSE)
pub fn library_filename<S: AsRef<OsStr>>(name: S) -> OsString {
    let name = name.as_ref();
    let mut string = OsString::with_capacity(name.len() + DLL_PREFIX.len() + DLL_SUFFIX.len());
    string.push(DLL_PREFIX);
    string.push(name);
    string.push(DLL_SUFFIX);
    string
}
