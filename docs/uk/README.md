# Slint Bindings

Вбудовує інтерфейси [Slint](https://slint.dev) у нативні застосунки: `NSView` / SwiftUI
на macOS і елемент WinUI 3 на Windows. Цикл подій належить хосту.
Пізніше те саме ядро малює дерева Weft (сусідній проєкт `weft`) нативно, а вебзбірка
використовує його знову через WebAssembly.

У Slint немає офіційної інтеграції зі Swift чи WinUI (див. [research.md](../../research.md)).
Підтримуваний спосіб вбудовування — власна платформа (`slint::platform`); на ній і зібрано проєкт.

**Статус:** M3. macOS показує кадр через `CAMetalLayer` (slintx-wgpu на Metal
у wgpu), WinUI — через `SwapChainPanel` (той самий рендерер на D3D12). Якщо
адаптер або поверхня не створюються, хост повертається до CPU-буфера.
Клавіатура, IME і DLL на кожен RID з M2 на місці. Поріг M3 за часом кадру
(4K швидше за 4 мс) тут не вимірювався, вікно WinUI не запускалося.

## Склад

| Шлях | Що це |
| --- | --- |
| `crates/slint-bindings-core` | Платформа Slint, якою керує хост: CPU-рендерер у RGBA-буфер викликача і slintx-wgpu в поверхню хоста. Введення, таймери, демо-компонент. |
| `crates/slint-bindings-ffi` | C ABI `sb_*` поверх ядра (статична бібліотека для Swift, DLL для WinUI). Лише пересилає виклики. |
| `flutter/` | Копія [slint_dart](https://github.com/listepo/slint_dart). Крейт `slint-embed` (`flutter/packages/slint/rust/embed`) — спільна програмна платформа, карта подій Dart FFI і перевірка UI-потоку. |
| `swift/` | Пакунок SwiftPM: `CSlintBindings` (модуль C над згенерованим заголовком), `SlintBindings` (`SlintHost`, `SlintNSView`, `SlintDemoView`), застосунок `SlintDemo`. |
| `windows/` | Елемент WinUI 3 `SlintPanel` і демо-застосунок. |

```text
SwiftUI / AppKit ─┐                       ┌─ WinUI 3 (C#)
  SlintDemoView   │   sb_* C ABI          │   SlintPanel
  SlintNSView     ├──▶ slint-bindings-ffi ◀┤   SlintHost (P/Invoke)
  SlintHost       │         │             │
                  │   slint-bindings-core │
                  │   CPU-буфер або       │
                  │   slintx-wgpu         │
```

## Як виходить кадр

1. Хост стежить за розміром і масштабом і викликає `sb_host_resize`.
2. На кожному тіку (`CADisplayLink` на macOS, `CompositionTarget.Rendering` у WinUI)
   хост викликає `sb_tick`, потім `sb_host_gpu_render` або `sb_host_render`.
   Slint перемальовує кадр лише коли щось змінилось.
3. GPU — це підшар `CAMetalLayer` на macOS (`sb_component_new_metal`) або
   `SwapChainPanel` у WinUI (`sb_component_new_swapchain`). Ланцюжок створює wgpu.
   Поки користувач тягне край вікна, у шару Metal увімкнено
   `presentsWithTransaction`, і кадр не розтягується. Якщо конструктор не
   вдався, хост показує CPU-кадр (вміст шару `CGImage` на macOS — RGBA8;
   `WriteableBitmap` у WinUI — BGRA8 з `sb_host_render_bgra`).
4. Миша, прокрутка, клавіатура і фокус повертаються через `sb_host_*`
   у логічних точках від лівого верхнього кута.

Усі виклики йдуть з UI-потоку хоста: об'єкти Slint однопотокові.

## Збірка і запуск (macOS, Apple Silicon)

```sh
mise install      # Rust 1.99, cargo-nextest, just; cbindgen через `cargo install cbindgen`
just check        # fmt, clippy, тести, дрейф заголовка, дрейф P/Invoke, swift build, swift test
just swift-run    # відкриває вікно демо SwiftUI
```

Windows: див. [windows/README.md](windows/README.md).

## Документи

- [plan.md](../../plan.md): етапи й поточні задачі
- [research.md](../../research.md): як можна вбудувати Slint, з джерелами
- [toolchain.md](../../toolchain.md): програми й пакунки
- [English](../../README.md), [Русский](../ru/README.md)

## Ліцензія

Apache-2.0. Див. [LICENSE](../../LICENSE).
