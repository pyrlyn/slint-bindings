# Хост Windows (WinUI 3)

Елемент повторює хост Swift. CI його збирає (`dotnet build` після
`just windows-dll`). Запуск вікна демо й далі потребує машини Windows.

| Проєкт | Що це |
| --- | --- |
| `SlintBindings.WinUI` | Елемент `SlintPanel`, `SlintHost` (володіє дескриптором Rust), оголошення P/Invoke |
| `SlintDemo.WinUI` | Розпаковане демо: панель WinUI і панель Slint зі спільним станом |

## Збірка (на Windows)

```powershell
mise install
just windows-dll
# ARM64: just windows-dll aarch64-pc-windows-msvc
dotnet build windows/SlintDemo.WinUI -p:Platform=x64
```

Збірка копіює `slint_bindings_ffi.dll` поруч із застосунком і в
`runtimes/win-x64/native` (або `win-arm64`, якщо `Platform=ARM64`). Обидва RID
потрапляють у вивід, якщо обидві DLL уже зібрані.

## Шлях малювання

- **GPU:** `SwapChainPanel`. Rust створює ланцюжок DXGI (wgpu
  `SurfaceTargetUnsafe::SwapChainPanel`, slintx-wgpu) і викликає
  `ISwapChainPanelNative::SetSwapChain` в UI-потоці. `CompositionScaleChanged`
  змінює розмір.
- **Запасний CPU:** кадри → `WriteableBitmap` → `Image`, якщо вказівник панелі
  або адаптер D3D12 недоступний.
- **Не планується:** дочірній `HWND`. У WinUI 3 немає підтримуваного елемента для нього,
  а композиція малює XAML поверх дочірніх вікон.

## TODO

- [x] Збірка в CI Windows (M2). Запуск вікна й далі ручний.
- [x] `just pinvoke-check` тримає `NativeMethods.cs` на експортах `sb_*` заголовка.
      Маршалінг лишається написаним вручну.
- [x] `VirtualKey` → текст клавіші Slint, включно з Shift+Tab, обома сторонами модифікаторів
      і F1–F24. Таблиця в Rust і покриває кожен `slint::platform::Key`.
- [x] IME через `CoreTextEditContext`, коли ОС його дає. У стільниці Windows 10
      немає CoreWindow, виклик падає, і текст як і раніше вставляє `CharacterReceived`.
- [x] BGRA з ядра (`sb_host_render_bgra`).
- [x] Копіювання DLL на кожен RID (`runtimes/win-x64/native`, `runtimes/win-arm64/native`).
- [x] Шлях GPU через `SwapChainPanel` (M3); бітмап CPU, якщо він не стартує.
      Час кадру не вимірювався.
- [ ] Пакунок NuGet і MSIX для демо (M6). Каталоги RID — це розкладка, яку забере пакування.
