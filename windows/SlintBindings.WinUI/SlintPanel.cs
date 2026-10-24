// WinUI 3 control presenting a SlintHost.
// The GPU path presents into a SwapChainPanel (wgpu creates the DXGI swap chain
// and calls ISwapChainPanelNative::SetSwapChain on this UI thread). When that
// surface cannot be created, frames are copied into a WriteableBitmap.

using System;
using System.Runtime.InteropServices;
using System.Runtime.InteropServices.WindowsRuntime;
using Microsoft.UI.Input;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using Windows.System;
using Windows.UI.Core;

namespace SlintBindings.WinUI;

public sealed class SlintPanel : UserControl
{
    /// <summary>WinUI wheel units are multiples of 120; Slint wants logical points.</summary>
    private const float WheelDeltaToPoints = 40f / 120f;

    private readonly Image _image = new() { Stretch = Stretch.Fill };
    private readonly SwapChainPanel _swapChain = new();
    private readonly SlintTextInput _textInput;
    private WriteableBitmap? _bitmap;
    private byte[] _bgra = [];
    private XamlRoot? _xamlRoot;

    public SlintHost? Host { get; private set; }

    /// <summary>What the panel shows. Set it, or <see cref="HostFactory"/>, before the panel loads.</summary>
    public SlintDefinition? Definition { get; set; }

    /// <summary>
    /// Makes the host instead of <see cref="Definition"/>, for a host another
    /// library builds (wrap its pointer with <see cref="SlintHost.Adopt"/>).
    /// The argument is an <c>ISwapChainPanelNative</c> pointer to present into,
    /// or zero for a CPU host. A CPU host returned for a pointer is kept; a
    /// throw for a pointer asks again with zero.
    /// </summary>
    public Func<IntPtr, SlintHost>? HostFactory { get; set; }
    public string? LastError { get; private set; }

    public SlintPanel()
    {
        _textInput = new SlintTextInput(() => Host, () => SlintTextInput.ScreenCaret(this));
        var root = new Grid();
        _swapChain.Visibility = Visibility.Collapsed;
        _swapChain.CompositionScaleChanged += (_, _) => SyncSize();
        root.Children.Add(_image);
        root.Children.Add(_swapChain);
        Content = root;
        IsTabStop = true;
        Loaded += OnLoaded;
        Unloaded += OnUnloaded;
        SizeChanged += (_, _) => SyncSize();
        PointerMoved += (_, e) => Forward(e, (x, y, _) => Host?.PointerMoved(x, y));
        PointerPressed += (_, e) =>
        {
            Focus(FocusState.Pointer);
            CapturePointer(e.Pointer);
            Forward(e, (x, y, b) => Host?.PointerPressed(x, y, b));
        };
        PointerReleased += (_, e) =>
        {
            Forward(e, (x, y, b) => Host?.PointerReleased(x, y, b));
            ReleasePointerCapture(e.Pointer);
        };
        PointerCanceled += (_, e) =>
        {
            Host?.PointerExited();
            ReleasePointerCapture(e.Pointer);
        };
        PointerExited += (_, _) => Host?.PointerExited();
        PointerWheelChanged += OnWheel;
        GotFocus += (_, _) =>
        {
            Host?.FocusChanged(true);
            _textInput.FocusEnter();
        };
        LostFocus += (_, _) =>
        {
            _textInput.FocusLeave();
            Host?.FocusChanged(false);
        };
        PreviewKeyDown += OnPreviewKeyDown;
        KeyUp += OnKeyUp;
        CharacterReceived += OnCharacterReceived;
    }

    private float PixelScale => (float)(XamlRoot?.RasterizationScale ?? 1.0);

    private void OnLoaded(object sender, RoutedEventArgs e)
    {
        try
        {
            Host ??= OpenHost(HostFactory ?? FactoryFor(
                Definition ?? throw new SlintException("Set SlintPanel.Definition or SlintPanel.HostFactory")));
        }
        catch (SlintException ex)
        {
            LastError = ex.Message;
            return;
        }
        _xamlRoot = XamlRoot;
        if (_xamlRoot is not null)
            _xamlRoot.Changed += OnXamlRootChanged;
        _textInput.Attach();
        SyncSize();
        CompositionTarget.Rendering += OnRendering;
    }

    private void OnUnloaded(object sender, RoutedEventArgs e)
    {
        if (_xamlRoot is not null)
            _xamlRoot.Changed -= OnXamlRootChanged;
        _xamlRoot = null;
        CompositionTarget.Rendering -= OnRendering;
        _textInput.Detach();
        Host?.Dispose();
        Host = null;
    }

    private void OnXamlRootChanged(XamlRoot sender, XamlRootChangedEventArgs args) => SyncSize();

    private void SyncSize()
    {
        if (Host is null) return;
        var w = Math.Max(1, (int)Math.Round(ActualWidth * PixelScale));
        var h = Math.Max(1, (int)Math.Round(ActualHeight * PixelScale));
        try
        {
            Host.Resize(w, h, PixelScale);
            if (Host.IsGpu)
            {
                RenderFrame();
                return;
            }
            if (_bitmap is null || _bitmap.PixelWidth != w || _bitmap.PixelHeight != h)
            {
                _bitmap = new WriteableBitmap(w, h);
                _image.Source = _bitmap;
            }
            RenderFrame();
        }
        catch (SlintException ex)
        {
            LastError = ex.Message;
        }
    }

    private void OnRendering(object? sender, object e)
    {
        SlintHost.Tick();
        RenderFrame();
    }

    private Func<IntPtr, SlintHost> FactoryFor(SlintDefinition definition) =>
        panel => new SlintHost(definition, 1, 1, PixelScale, panel);

    private SlintHost OpenHost(Func<IntPtr, SlintHost> factory)
    {
        IntPtr native = 0;
        try
        {
            native = Marshal.GetComInterfaceForObject<SwapChainPanel, ISwapChainPanelNative>(_swapChain);
            var host = factory(native);
            _image.Visibility = host.IsGpu ? Visibility.Collapsed : Visibility.Visible;
            _swapChain.Visibility = host.IsGpu ? Visibility.Visible : Visibility.Collapsed;
            return host;
        }
        catch (Exception ex)
        {
            LastError = ex.Message;
        }
        finally
        {
            if (native != 0) Marshal.Release(native);
        }
        _swapChain.Visibility = Visibility.Collapsed;
        _image.Visibility = Visibility.Visible;
        return factory(IntPtr.Zero);
    }

    private void RenderFrame()
    {
        if (Host is null) return;
        if (Host.IsGpu)
        {
            try
            {
                Host.RenderGpu(out _);
            }
            catch (SlintException ex)
            {
                LastError = ex.Message;
            }
            return;
        }
        if (_bitmap is null) return;
        try
        {
            var needed = _bitmap.PixelWidth * _bitmap.PixelHeight * 4;
            if (_bgra.Length != needed) _bgra = new byte[needed];
            if (Host.RenderBgra(_bgra, out _))
            {
                using var stream = _bitmap.PixelBuffer.AsStream();
                stream.Write(_bgra, 0, _bgra.Length);
                _bitmap.Invalidate();
            }
        }
        catch (SlintException ex)
        {
            LastError = ex.Message;
        }
    }

    private void OnWheel(object sender, PointerRoutedEventArgs e)
    {
        var p = e.GetCurrentPoint(this);
        var delta = p.Properties.MouseWheelDelta * WheelDeltaToPoints;
        var dx = p.Properties.IsHorizontalMouseWheel ? delta : 0;
        var dy = p.Properties.IsHorizontalMouseWheel ? 0 : delta;
        Host?.PointerScrolled((float)p.Position.X, (float)p.Position.Y, dx, dy);
        e.Handled = true;
    }

    private void OnPreviewKeyDown(object sender, KeyRoutedEventArgs e)
    {
        _textInput.NoteKeyDown();
        // While a composition is open, Backspace, Enter, Escape and the arrows
        // belong to the input method (delete a reading, confirm, move a candidate).
        if (_textInput.IsComposing && ImeOwns(e.Key)) return;

        var shift = IsDown(VirtualKey.Shift) || IsDown(VirtualKey.LeftShift) || IsDown(VirtualKey.RightShift);
        var control = IsDown(VirtualKey.Control) || IsDown(VirtualKey.LeftControl) || IsDown(VirtualKey.RightControl);
        var text = SlintKeys.Command(e.Key, shift);
        if (text is null && control)
            text = SlintKeys.Text(e.Key, "", shift, true);
        if (text is null) return;

        if (IsModifier(e.Key))
        {
            if (!e.KeyStatus.WasKeyDown) Host?.KeyPressed(text);
        }
        else if (e.KeyStatus.WasKeyDown)
        {
            Host?.KeyRepeated(text);
        }
        else
        {
            var accepted = Host?.KeyPressed(text) ?? false;
            if (!accepted && text is "\t" or "\u0019")
                MoveFocus(text == "\t");
        }
        e.Handled = true;
    }

    private void OnKeyUp(object sender, KeyRoutedEventArgs e)
    {
        if (_textInput.IsComposing && ImeOwns(e.Key)) return;
        var control = IsDown(VirtualKey.Control) || IsDown(VirtualKey.LeftControl) || IsDown(VirtualKey.RightControl)
            || e.Key is VirtualKey.Control or VirtualKey.LeftControl or VirtualKey.RightControl;
        var text = SlintKeys.Command(e.Key, false);
        if (text is null && control)
            text = SlintKeys.Text(e.Key, "", false, true);
        if (text is null) return;
        Host?.KeyReleased(text);
        e.Handled = true;
    }

    private void OnCharacterReceived(object sender, CharacterReceivedRoutedEventArgs e)
    {
        if (_textInput.ConsumeCharacter())
        {
            e.Handled = true;
            return;
        }
        if (char.IsControl(e.Character) && e.Character is not ' ') return;
        var text = SlintKeys.Text((VirtualKey)0, e.Character.ToString(), false, false);
        if (text is null) return;
        if (e.KeyStatus.WasKeyDown) Host?.KeyRepeated(text);
        else
        {
            Host?.KeyPressed(text);
            Host?.KeyReleased(text);
        }
        e.Handled = true;
    }

    private static bool IsDown(VirtualKey key)
    {
        var state = InputKeyboardSource.GetKeyStateForCurrentThread(key);
        return state.HasFlag(CoreVirtualKeyStates.Down);
    }

    private static bool IsModifier(VirtualKey key) => key is
        VirtualKey.Shift or VirtualKey.LeftShift or VirtualKey.RightShift or
        VirtualKey.Control or VirtualKey.LeftControl or VirtualKey.RightControl or
        VirtualKey.Menu or VirtualKey.LeftMenu or VirtualKey.RightMenu or
        VirtualKey.LeftWindows or VirtualKey.RightWindows or
        VirtualKey.CapitalLock;

    private static bool ImeOwns(VirtualKey key) => key is
        VirtualKey.Back or VirtualKey.Escape or VirtualKey.Enter or
        VirtualKey.Space or VirtualKey.Left or VirtualKey.Right or
        VirtualKey.Up or VirtualKey.Down;

    private void MoveFocus(bool forward)
    {
        var direction = forward ? FocusNavigationDirection.Next : FocusNavigationDirection.Previous;
        _ = FocusManager.TryMoveFocus(direction);
    }

    // Positions arrive in DIPs (logical pixels) relative to this control, which is what Slint expects.
    private void Forward(PointerRoutedEventArgs e, Action<float, float, int> send)
    {
        var p = e.GetCurrentPoint(this);
        var button = p.Properties.PointerUpdateKind switch
        {
            PointerUpdateKind.RightButtonPressed or PointerUpdateKind.RightButtonReleased => 1,
            PointerUpdateKind.MiddleButtonPressed or PointerUpdateKind.MiddleButtonReleased => 2,
            _ => 0,
        };
        send((float)p.Position.X, (float)p.Position.Y, button);
        e.Handled = true;
    }
}

/// WinUI 3's swap-chain panel, as the COM interface wgpu's DX12 backend takes.
[ComImport]
[Guid("63aad0b8-7c24-40ff-85a8-640d944cc325")]
[InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface ISwapChainPanelNative
{
    void SetSwapChain(IntPtr swapChain);
}
