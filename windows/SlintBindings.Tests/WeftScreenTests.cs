// A screen weft's `weft-slint` generated (fixtures/weft/login.slint), driven
// through the C# host the way SlintPanel forwards WinUI input to it.
// Each platform's fonts move its rows, so targets are found by column at 480
// logical px wide: down x 100 lie the email field, the password field, the
// submit button and "Forgot password?"; up x 440 from the bottom, "Create an
// account" comes first.

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using Microsoft.VisualStudio.TestTools.UnitTesting;
using SlintBindings.WinUI;

namespace SlintBindings.Tests;

[TestClass]
public sealed class WeftScreenTests
{
    private const int Width = 480;
    private const int Height = 800;
    private const float Left = 100;
    private const float Right = 440;
    private const string Reset = """["nav.reset","reset"]""";
    private const string Submit = """["auth.submit","form"]""";
    private const string Signup = """["nav.signup","signup"]""";
    private static readonly string Fixture = Path.Combine(AppContext.BaseDirectory, "login.slint");

    [TestMethod]
    public void GeneratedScreenShowsAndTakesInput()
    {
        using var definition = new SlintDefinition(Fixture);
        using var host = new SlintHost(definition, Width, Height, 1);
        AssertRendersSomething(host);
        var performed = new List<string>();
        host.OnCallback("perform", performed.Add);
        host.FocusChanged(true);

        // The submit button is disabled while the email is empty, so the
        // column's first callback is the reset link below it. The walk ends
        // with the password field focused.
        Assert.IsTrue(ClickUntilPerformed(host, performed, Left, Down()));
        Type(host, "secret");
        Assert.AreEqual("\"secret\"", host.GetProperty("password"));
        Type(host, "\u0019"); // Backtab, to the email field.
        Type(host, "ada@example.com");
        Assert.AreEqual("\"ada@example.com\"", host.GetProperty("email"));

        // Enter in the field submits the form (`accepted`); so does the button.
        Type(host, "\n");
        Assert.IsTrue(ClickUntilPerformed(host, performed, Left, Down()));
        Assert.IsTrue(ClickUntilPerformed(host, performed, Right, Down().Reverse()));
        CollectionAssert.AreEqual(new[] { Reset, Submit, Submit, Signup }, performed);
    }

    [TestMethod]
    public void AdoptedHostTakesItsSizeFromTheHost()
    {
        using var definition = new SlintDefinition(Fixture);
        // Stands in for a library such as weft's `ws_surface_new`: a raw `sb_*`
        // call hands C# an `SbHost*` it did not create.
        using var host = SlintHost.Adopt(NativeMethods.sb_component_new(definition.Handle, Width, Height, 1));
        Assert.AreEqual(Width, host.PixelWidth);
        Assert.AreEqual(Height, host.PixelHeight);
        Assert.IsFalse(host.IsGpu);
        AssertRendersSomething(host);
    }

    [TestMethod]
    public void AdoptingAFailedConstructorThrowsItsError()
    {
        var error = Assert.ThrowsExactly<SlintException>(
            () => SlintHost.Adopt(NativeMethods.sb_component_new(IntPtr.Zero, 1, 1, 1)));
        Assert.Contains("null", error.Message);
    }

    private static IEnumerable<int> Down() => Enumerable.Range(0, Height / 2).Select(i => i * 2);

    /// <summary>Clicks along the column at <paramref name="x"/> until a callback lands in <paramref name="performed"/>.</summary>
    private static bool ClickUntilPerformed(SlintHost host, List<string> performed, float x, IEnumerable<int> ys)
    {
        var before = performed.Count;
        return ys.Any(y =>
        {
            host.PointerMoved(x, y);
            host.PointerPressed(x, y, 0);
            host.PointerReleased(x, y, 0);
            return performed.Count > before;
        });
    }

    private static void AssertRendersSomething(SlintHost host)
    {
        var bgra = new byte[host.PixelWidth * host.PixelHeight * 4];
        Assert.IsTrue(host.RenderBgra(bgra, out _));
        var pixels = MemoryMarshal.Cast<byte, uint>(bgra);
        Assert.IsTrue(pixels.ContainsAnyExcept(pixels[0]), "the frame is one colour");
    }

    private static void Type(SlintHost host, string text)
    {
        foreach (var c in text)
        {
            Assert.IsTrue(host.KeyPressed(c.ToString()), $"key {(int)c} was not taken");
            host.KeyReleased(c.ToString());
        }
    }
}
