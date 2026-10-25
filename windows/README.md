# Windows host (WinUI 3)

The control mirrors the Swift host. CI runs the host tests, builds the demo
and opens it on the UI description format login screen (`fixtures/weft/login.slint`), failing
when the Slint panel is one colour (`smoke.ps1`).

| Project | What it is |
| --- | --- |
| `SlintBindings.WinUI` | `SlintPanel` control, `SlintHost` (owns the Rust handle), P/Invoke declarations |
| `SlintDemo.WinUI` | Unpackaged demo app: a WinUI pane and a Slint pane sharing state. A `.slint` path argument shows that file instead and lists its `perform` calls |
| `SlintBindings.Tests` | MSTest: the UI description format screen through `SlintHost` (render, typing, clicks, callbacks, adoption). Compiles the host's sources, so it needs no Windows App Runtime |

## Build (on Windows)

```powershell
mise install
just windows-dll
# ARM64: just windows-dll aarch64-pc-windows-msvc
dotnet build windows/SlintDemo.WinUI -p:Platform=x64
dotnet run --project windows/SlintBindings.Tests
```

The build copies `slint_bindings_ffi.dll` next to the app and under
`runtimes/win-x64/native` (or `win-arm64` when `Platform=ARM64`). Both RIDs
are staged when both DLLs have been built.

## A host from another library

A library that makes its own `SbHost*` (the UI description format's `ws_surface_new`, say) hands it
to the panel through `HostFactory`; `SlintHost.Adopt` takes ownership and reads
the size from the host. The factory gets the swap chain pointer, or zero for a
CPU host, and is asked again with zero when it throws for the pointer.

```csharp
panel.HostFactory = swapChain => SlintHost.Adopt(MakeNativeHost(swapChain));
```

## Rendering path

- **GPU:** `SwapChainPanel`. Rust creates the DXGI swap chain (wgpu
  `SurfaceTargetUnsafe::SwapChainPanel`, slintx-wgpu) and calls
  `ISwapChainPanelNative::SetSwapChain` on the UI thread. `CompositionScaleChanged`
  resizes it.
- **CPU fallback:** frames → `WriteableBitmap` → `Image`, used when the panel
  pointer or the D3D12 adapter is not available.
- **Not planned:** a child `HWND`. WinUI 3 has no supported control for hosting
  one, and composition draws XAML over child windows.

## TODO

- [x] Build on Windows CI (M2), plus the host tests and a window smoke run on
      the weft screen (T71). Typing into the window is still manual.
- [x] `just pinvoke-check` keeps `NativeMethods.cs` on the header's `sb_*` exports.
      Marshalling stays hand-written.
- [x] `VirtualKey` → Slint key text, including Shift+Tab, both modifier sides,
      and F1–F24. The table is in Rust and covers every `slint::platform::Key`.
- [x] IME via `CoreTextEditContext` when the OS provides one. Windows 10 desktop
      has no CoreWindow, so that call fails and `CharacterReceived` still inserts text.
- [x] BGRA output from the core (`sb_host_render_bgra`).
- [x] Per-RID DLL copy (`runtimes/win-x64/native`, `runtimes/win-arm64/native`).
- [x] GPU path via `SwapChainPanel` (M3), with the CPU bitmap when it cannot start.
      The frame-time checkpoint has not been measured.
- [ ] NuGet package and MSIX for the demo (M6). The RID folders are the layout that pack will use.
