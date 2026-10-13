// P/Invoke declarations for the sb_* C ABI (swift/Sources/CSlintBindings/slint_bindings.h).
// `just pinvoke-check` fails when an sb_* export is missing here or invented here.
// Marshalling stays hand-written: csbindgen is not a dependency of this repo.

using System;
using System.Runtime.InteropServices;

namespace SlintBindings.WinUI;

[StructLayout(LayoutKind.Sequential)]
internal struct SbFrame
{
    public byte Redrawn;
    public byte Animating;
    public byte Failed;
}

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
internal delegate void SbSubmittedFn(IntPtr userData, IntPtr name);

// `argsJson` is a UTF-8 JSON array, valid only during the call.
[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
internal delegate void SbCallbackFn(IntPtr userData, IntPtr argsJson);

internal static partial class NativeMethods
{
    private const string Lib = "slint_bindings_ffi";

    [LibraryImport(Lib)] internal static partial IntPtr sb_last_error();

    // `source` null reads the file at `path`.
    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8)]
    internal static partial IntPtr sb_compile(string? source, string path);
    [LibraryImport(Lib)] internal static partial void sb_definition_free(IntPtr definition);
    [LibraryImport(Lib)] internal static partial IntPtr sb_component_new(IntPtr definition, uint width, uint height, float scale);
    [LibraryImport(Lib)] internal static partial IntPtr sb_component_new_metal(IntPtr definition, IntPtr layer, uint width, uint height, float scale);
    [LibraryImport(Lib)] internal static partial IntPtr sb_component_new_swapchain(IntPtr definition, IntPtr panel, uint width, uint height, float scale);

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8)]
    [return: MarshalAs(UnmanagedType.U1)]
    internal static partial bool sb_set_property_json(IntPtr host, string name, string json);

    // Returns an owned UTF-8 string (free with sb_string_free) or null.
    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8)]
    internal static partial IntPtr sb_get_property_json(IntPtr host, string name);

    // Returns an owned UTF-8 string (free with sb_string_free) or null.
    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8)]
    internal static partial IntPtr sb_invoke_json(IntPtr host, string name, string argsJson);

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8)]
    [return: MarshalAs(UnmanagedType.U1)]
    internal static partial bool sb_on_callback(IntPtr host, string name, IntPtr callback, IntPtr userData);

    [LibraryImport(Lib)] internal static partial void sb_string_free(IntPtr text);
    [LibraryImport(Lib)] internal static partial IntPtr sb_demo_new(uint width, uint height, float scale);
    [LibraryImport(Lib)] internal static partial IntPtr sb_demo_new_metal(IntPtr layer, uint width, uint height, float scale);
    [LibraryImport(Lib)] internal static partial IntPtr sb_demo_new_swapchain(IntPtr panel, uint width, uint height, float scale);

    [LibraryImport(Lib)]
    [return: MarshalAs(UnmanagedType.U1)]
    internal static partial bool sb_host_is_gpu(IntPtr host);

    [LibraryImport(Lib)] internal static partial SbFrame sb_host_gpu_render(IntPtr host);
    [LibraryImport(Lib)] internal static partial void sb_host_free(IntPtr host);
    [LibraryImport(Lib)] internal static partial nuint sb_host_frame_len(IntPtr host);

    [LibraryImport(Lib)]
    [return: MarshalAs(UnmanagedType.U1)]
    internal static partial bool sb_host_resize(IntPtr host, uint width, uint height, float scale);

    [LibraryImport(Lib)] internal static unsafe partial SbFrame sb_host_render(IntPtr host, byte* buf, nuint len);
    [LibraryImport(Lib)] internal static unsafe partial SbFrame sb_host_render_bgra(IntPtr host, byte* buf, nuint len);
    [LibraryImport(Lib)] internal static partial void sb_tick();

    [LibraryImport(Lib)]
    [return: MarshalAs(UnmanagedType.U1)]
    internal static partial bool sb_host_pointer_moved(IntPtr host, float x, float y);

    [LibraryImport(Lib)]
    [return: MarshalAs(UnmanagedType.U1)]
    internal static partial bool sb_host_pointer_pressed(IntPtr host, float x, float y, byte button);

    [LibraryImport(Lib)]
    [return: MarshalAs(UnmanagedType.U1)]
    internal static partial bool sb_host_pointer_released(IntPtr host, float x, float y, byte button);

    [LibraryImport(Lib)]
    [return: MarshalAs(UnmanagedType.U1)]
    internal static partial bool sb_host_pointer_exited(IntPtr host);

    [LibraryImport(Lib)]
    [return: MarshalAs(UnmanagedType.U1)]
    internal static partial bool sb_host_pointer_scrolled(IntPtr host, float x, float y, float dx, float dy);

    [LibraryImport(Lib)]
    [return: MarshalAs(UnmanagedType.U1)]
    internal static partial bool sb_host_focus_changed(IntPtr host, [MarshalAs(UnmanagedType.U1)] bool focused);

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8)]
    [return: MarshalAs(UnmanagedType.U1)]
    internal static partial bool sb_host_key_pressed(IntPtr host, string text);

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8)]
    [return: MarshalAs(UnmanagedType.U1)]
    internal static partial bool sb_host_key_released(IntPtr host, string text);

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8)]
    [return: MarshalAs(UnmanagedType.U1)]
    internal static partial bool sb_host_key_repeated(IntPtr host, string text);

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8)]
    [return: MarshalAs(UnmanagedType.U1)]
    internal static partial bool sb_host_composition_update(IntPtr host, string preedit, int utf16Start, int utf16End);

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8)]
    [return: MarshalAs(UnmanagedType.U1)]
    internal static partial bool sb_host_composition_commit(IntPtr host, string text);

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8)]
    [return: MarshalAs(UnmanagedType.U1)]
    internal static unsafe partial bool sb_appkit_key_text(
        ushort keyCode, string characters, string ignoring, uint modifiers, byte* buffer, nuint bufferLen);

    [LibraryImport(Lib)]
    [return: MarshalAs(UnmanagedType.U1)]
    internal static unsafe partial bool sb_virtual_key_command(
        ushort virtualKey, [MarshalAs(UnmanagedType.U1)] bool shift, byte* buffer, nuint bufferLen);

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8)]
    [return: MarshalAs(UnmanagedType.U1)]
    internal static unsafe partial bool sb_virtual_key_text(
        ushort virtualKey, string character, [MarshalAs(UnmanagedType.U1)] bool shift,
        [MarshalAs(UnmanagedType.U1)] bool control, byte* buffer, nuint bufferLen);

    [LibraryImport(Lib)]
    [return: MarshalAs(UnmanagedType.U1)]
    internal static partial bool sb_host_ime_started(IntPtr host);

    [LibraryImport(Lib)]
    [return: MarshalAs(UnmanagedType.U1)]
    internal static partial bool sb_host_ime_composing(IntPtr host);

    [LibraryImport(Lib)]
    [return: MarshalAs(UnmanagedType.U1)]
    internal static unsafe partial bool sb_host_ime_text(IntPtr host, byte* buffer, nuint bufferLen);

    [LibraryImport(Lib)]
    [return: MarshalAs(UnmanagedType.U1)]
    internal static unsafe partial bool sb_host_ime_selection(IntPtr host, int* start, int* end);

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8)]
    [return: MarshalAs(UnmanagedType.U1)]
    internal static partial bool sb_host_ime_replace(
        IntPtr host, int rangeStart, int rangeEnd, string text, int selStart, int selEnd);

    [LibraryImport(Lib)]
    [return: MarshalAs(UnmanagedType.U1)]
    internal static partial bool sb_host_ime_select(IntPtr host, int start, int end);

    [LibraryImport(Lib)]
    [return: MarshalAs(UnmanagedType.U1)]
    internal static partial bool sb_host_ime_completed(IntPtr host, [MarshalAs(UnmanagedType.U1)] bool canceled);

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8)]
    internal static partial void sb_demo_set_name(IntPtr host, string name);

    [LibraryImport(Lib)]
    internal static partial void sb_demo_on_submitted(IntPtr host, IntPtr callback, IntPtr userData);

    internal static string LastError() =>
        Marshal.PtrToStringUTF8(sb_last_error()) ?? "unknown error";
}
