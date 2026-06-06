//! Path-A probe (dev tooling): does the tract `deep_filter` runtime actually LOAD the bundled
//! DeepFilterNet3 model on this toolchain? Build with `--features dfn`. Prints OK or the load error.
//!
//!   cargo run -p sukoon-core --features dfn --example dfn_probe

fn main() {
    #[cfg(feature = "dfn")]
    {
        use ndarray_dfn::Array2;
        match df::tract::DfTract::new(
            df::tract::DfParams::default(),
            &df::tract::RuntimeParams::default_with_ch(1),
        ) {
            Ok(mut m) => {
                println!("PATH_A_LOAD_OK: DfTract constructed (hop={})", m.hop_size);
                // Try one frame of silence through process() to confirm it actually runs.
                let hop = m.hop_size;
                let noisy = Array2::<f32>::zeros((1, hop));
                let mut enh = Array2::<f32>::zeros((1, hop));
                match m.process(noisy.view(), enh.view_mut()) {
                    Ok(lsnr) => println!("PATH_A_RUN_OK: processed one frame, lsnr={lsnr}"),
                    Err(e) => println!("PATH_A_RUN_FAIL: {e:#}"),
                }
            }
            Err(e) => println!("PATH_A_LOAD_FAIL: {e:#}"),
        }
    }
    #[cfg(not(feature = "dfn"))]
    println!("build with --features dfn");
}
