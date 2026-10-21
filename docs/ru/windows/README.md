# Хост Windows (WinUI 3)

Элемент повторяет хост Swift. CI его собирает (`dotnet build` после
`just windows-dll`). Запуск окна демо по-прежнему требует машину Windows.

| Проект | Что это |
| --- | --- |
| `SlintBindings.WinUI` | Элемент `SlintPanel`, `SlintHost` (владеет дескриптором Rust), объявления P/Invoke |
| `SlintDemo.WinUI` | Распакованное демо: панель WinUI и панель Slint с общим состоянием |

## Сборка (на Windows)

```powershell
mise install
just windows-dll
# ARM64: just windows-dll aarch64-pc-windows-msvc
dotnet build windows/SlintDemo.WinUI -p:Platform=x64
```

Сборка копирует `slint_bindings_ffi.dll` рядом с приложением и в
`runtimes/win-x64/native` (или `win-arm64`, если `Platform=ARM64`). Оба RID
попадают в вывод, если обе DLL уже собраны.

## Путь отрисовки

- **GPU:** `SwapChainPanel`. Rust создаёт цепочку DXGI (wgpu
  `SurfaceTargetUnsafe::SwapChainPanel`, slintx-wgpu) и вызывает
  `ISwapChainPanelNative::SetSwapChain` в UI-потоке. `CompositionScaleChanged`
  меняет размер.
- **Запасной CPU:** кадры → `WriteableBitmap` → `Image`, если указатель панели
  или адаптер D3D12 недоступен.
- **Не планируется:** дочерний `HWND`. У WinUI 3 нет поддерживаемого элемента для него,
  а композиция рисует XAML поверх дочерних окон.

## TODO

- [x] Сборка в CI Windows (M2). Запуск окна всё ещё ручной.
- [x] `just pinvoke-check` держит `NativeMethods.cs` на экспортах `sb_*` заголовка.
      Маршалинг остаётся написанным вручную.
- [x] `VirtualKey` → текст клавиши Slint, включая Shift+Tab, обе стороны модификаторов
      и F1–F24. Таблица в Rust и покрывает каждый `slint::platform::Key`.
- [x] IME через `CoreTextEditContext`, когда ОС его даёт. У рабочего стола Windows 10
      нет CoreWindow, вызов падает, и текст по-прежнему вставляет `CharacterReceived`.
- [x] BGRA из ядра (`sb_host_render_bgra`).
- [x] Копирование DLL на каждый RID (`runtimes/win-x64/native`, `runtimes/win-arm64/native`).
- [x] Путь GPU через `SwapChainPanel` (M3); битмап CPU, если он не стартует.
      Время кадра не измерялось.
- [ ] Пакет NuGet и MSIX для демо (M6). Каталоги RID — это раскладка, которую заберёт упаковка.
