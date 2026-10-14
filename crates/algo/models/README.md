# DTF Craft super-resolution model

`realesr-general-x4v3.dtfsr` holds the weights of Real-ESRGAN's compact general model
`realesr-general-x4v3` (SRVGGNetCompact: 64 features, 32 convolutions, PReLU, ×4), converted from
the official `realesr-general-x4v3.pth` of the Real-ESRGAN v0.2.5.0 release
(https://github.com/xinntao/Real-ESRGAN) to a flat little-endian layout read by
`src/upscale.rs` (`DTFSR1`).

Real-ESRGAN is © 2021 Xintao Wang and released under the BSD 3-Clause License (see `LICENSE`).

`test/ref_in.f32` / `test/ref_out.f32` are a 32 × 24 RGB crop and its ×4 result computed with an
independent NumPy implementation of the network, used by the unit tests.
