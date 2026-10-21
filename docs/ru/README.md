# Slint Bindings

Встраивает интерфейсы [Slint](https://slint.dev) в нативные приложения: `NSView` / SwiftUI
на macOS и элемент WinUI 3 на Windows. Цикл событий принадлежит хосту.
Позже то же ядро рисует деревья Weft (соседний проект `weft`) нативно, а веб-сборка
переиспользует его через WebAssembly.

У Slint нет официальной интеграции со Swift или WinUI (см. [research.md](../../research.md)).
Поддерживаемый способ встраивания — своя платформа (`slint::platform`); на ней и построен проект.

**Статус:** M3. macOS показывает кадр через `CAMetalLayer` (slintx-wgpu на Metal
у wgpu), WinUI — через `SwapChainPanel` (тот же рендерер на D3D12). Если
адаптер или поверхность не создаются, хост возвращается к CPU-буферу.
Клавиатура, IME и DLL на каждый RID из M2 на месте. Порог M3 по времени кадра
(4K быстрее 4 мс) здесь не измерялся, окно WinUI не запускалось.

## Состав

| Путь | Что это |
| --- | --- |
| `crates/slint-bindings-core` | Платформа Slint, которой управляет хост: CPU-рендерер в RGBA-буфер вызывающего и slintx-wgpu в поверхность хоста. Ввод, таймеры, демо-компонент. |
| `crates/slint-bindings-ffi` | C ABI `sb_*` поверх ядра (статическая библиотека для Swift, DLL для WinUI). Только пересылает вызовы. |
| `flutter/` | Копия [slint_dart](https://github.com/listepo/slint_dart). Крейт `slint-embed` (`flutter/packages/slint/rust/embed`) — общая программная платформа, карта событий Dart FFI и проверка UI-потока. |
| `swift/` | Пакет SwiftPM: `CSlintBindings` (модуль C над сгенерированным заголовком), `SlintBindings` (`SlintHost`, `SlintNSView`, `SlintDemoView`), приложение `SlintDemo`. |
| `windows/` | Элемент WinUI 3 `SlintPanel` и демо-приложение. |

```text
SwiftUI / AppKit ─┐                       ┌─ WinUI 3 (C#)
  SlintDemoView   │   sb_* C ABI          │   SlintPanel
  SlintNSView     ├──▶ slint-bindings-ffi ◀┤   SlintHost (P/Invoke)
  SlintHost       │         │             │
                  │   slint-bindings-core │
                  │   CPU-буфер или       │
                  │   slintx-wgpu         │
```

## Как получается кадр

1. Хост следит за размером и масштабом и вызывает `sb_host_resize`.
2. На каждом тике (`CADisplayLink` на macOS, `CompositionTarget.Rendering` в WinUI)
   хост вызывает `sb_tick`, затем `sb_host_gpu_render` или `sb_host_render`.
   Slint перерисовывает кадр только когда что-то изменилось.
3. GPU — это подслой `CAMetalLayer` на macOS (`sb_component_new_metal`) или
   `SwapChainPanel` в WinUI (`sb_component_new_swapchain`). Цепочку создаёт wgpu.
   Пока пользователь тянет край окна, у слоя Metal включён
   `presentsWithTransaction`, и кадр не растягивается. Если конструктор не
   удался, хост показывает CPU-кадр (содержимое слоя `CGImage` на macOS — RGBA8;
   `WriteableBitmap` в WinUI — BGRA8 из `sb_host_render_bgra`).
4. Мышь, прокрутка, клавиатура и фокус возвращаются через `sb_host_*`
   в логических точках от левого верхнего угла.

Все вызовы идут из UI-потока хоста: объекты Slint однопоточные.

## Сборка и запуск (macOS, Apple Silicon)

```sh
mise install      # Rust 1.99, cargo-nextest, just; cbindgen через `cargo install cbindgen`
just check        # fmt, clippy, тесты, дрейф заголовка, дрейф P/Invoke, swift build, swift test
just swift-run    # открывает окно демо SwiftUI
```

Windows: см. [windows/README.md](windows/README.md).

## Документы

- [plan.md](../../plan.md): этапы и текущие задачи
- [research.md](../../research.md): как можно встроить Slint, с источниками
- [toolchain.md](../../toolchain.md): программы и пакеты
- [English](../../README.md), [Українська](../uk/README.md)

## Лицензия

Apache-2.0. См. [LICENSE](../../LICENSE).
