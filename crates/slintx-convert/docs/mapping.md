# Slint to XML, HTML, Storyboard and Blueprint: mapping

How `slint-convert` maps each Slint construct to each output. The goal is the
same structure and meaning, not identical pixels. Anything an output cannot
express produces a warning with its source position and a comment in the
output. `--strict` turns those warnings into errors.

## Widgets

The table is generated from `src/mapping.rs`, and a unit test fails when it
goes stale. "Default stretch" is the `horizontal-stretch` and
`vertical-stretch` the converter assumes when the source sets none.

| Slint | HTML | UIKit | AppKit | GTK 4 | Default stretch (h, v) |
| --- | --- | --- | --- | --- | --- |
| `Window` | `main` | `UIView` | `NSView` | `Gtk.Window` | 1, 1 |
| `Rectangle` | `div` | `UIView` | `NSView` | `Gtk.Box` | 1, 1 |
| `Empty` | `div` | `UIView` | `NSView` | `Gtk.Box` | 1, 1 |
| `TouchArea` | `div` | `UIView` | `NSView` | `Gtk.Box` | 1, 1 |
| `FocusScope` | `div` | `UIView` | `NSView` | `Gtk.Box` | 1, 1 |
| `Flickable` | `div` | `UIScrollView` | `NSView` | `Gtk.ScrolledWindow` | 1, 1 |
| `ScrollView` | `div` | `UIScrollView` | `NSView` | `Gtk.ScrolledWindow` | 1, 1 |
| `HorizontalLayout` | `div` | `UIStackView` | `NSStackView` | `Gtk.Box` | 1, 1 |
| `VerticalLayout` | `div` | `UIStackView` | `NSStackView` | `Gtk.Box` | 1, 1 |
| `GridLayout` | `div` | `UIStackView` | `NSStackView` | `Gtk.Grid` | 1, 1 |
| `HorizontalBox` | `div` | `UIStackView` | `NSStackView` | `Gtk.Box` | 1, 1 |
| `VerticalBox` | `div` | `UIStackView` | `NSStackView` | `Gtk.Box` | 1, 1 |
| `GridBox` | `div` | `UIStackView` | `NSStackView` | `Gtk.Grid` | 1, 1 |
| `Row` | — | — | — | — | 1, 1 |
| `Text` | `span` | `UILabel` | `NSTextField` | `Gtk.Label` | 0, 0 |
| `Image` | `img` | `UIImageView` | `NSImageView` | `Gtk.Picture` | 0, 0 |
| `TextInput` | `input` | `UITextField` | `NSTextField` | `Gtk.Entry` | 1, 0 |
| `Button` | `button` | `UIButton` | `NSButton` | `Gtk.Button` | 0, 0 |
| `CheckBox` | `input` | `UISwitch` | `NSButton` | `Gtk.CheckButton` | 0, 0 |
| `Switch` | `input` | `UISwitch` | `NSSwitch` | `Gtk.Switch` | 0, 0 |
| `LineEdit` | `input` | `UITextField` | `NSTextField` | `Gtk.Entry` | 1, 0 |
| `TextEdit` | `textarea` | `UITextView` | `NSTextField` | `Gtk.TextView` | 1, 1 |
| `Slider` | `input` | `UISlider` | `NSSlider` | `Gtk.Scale` | 1, 0 |
| `ProgressIndicator` | `progress` | `UIProgressView` | `NSProgressIndicator` | `Gtk.ProgressBar` | 1, 0 |
| `Spinner` | `div` | `UIActivityIndicatorView` | `NSProgressIndicator` | `Gtk.Spinner` | 0, 0 |
| `SpinBox` | `input` | `UIStepper` | `NSStepper` | `Gtk.SpinButton` | 1, 0 |
| `ComboBox` | `select` | `UIButton` | `NSPopUpButton` | `Gtk.DropDown` | 1, 0 |
| `GroupBox` | `fieldset` | `UIView` | `NSView` | `Gtk.Frame` | 1, 1 |

The XML output has no table column because it keeps every Slint element name
as it is.

## Components

- The main component is the last exported one, or the last component when
  none is exported.
- Local components are inlined at each use. `root.x` and bare `x` inside them
  are replaced with the use-site value, or with the declared default.
- `@children` is replaced with the use-site children, at any depth.
- Elements imported from files other than `std-widgets.slint` are not
  resolved. They become an unsupported element with a warning.

## Layouts

| Slint | HTML | UIKit / AppKit | GTK 4 |
| --- | --- | --- | --- |
| `HorizontalLayout`, `VerticalLayout` | `display:flex`, `gap`, `padding` | stack view: `axis` or `orientation`, `spacing`, edge insets | `Gtk.Box`, `spacing`, margins |
| `GridLayout`, `Row`, `row`/`col`/`rowspan`/`colspan` | `display:grid`, `grid-row`/`grid-column` with spans | vertical stack of row stacks; columns tied by equal-width constraints; empty cells become placeholder views | `Gtk.Grid` with `layout { row; column; row-span; column-span; }` |
| `spacing`, `spacing-horizontal`, `spacing-vertical` | `gap`, `row-gap`, `column-gap` | stack `spacing` | `spacing`, `row-spacing`, `column-spacing` |
| `padding*` | `padding` | `directionalLayoutMargins` (UIKit), `edgeInsets` (AppKit) | `margin-*` on the box |
| `alignment: stretch` | default flex | `distribution="fill"` | `hexpand`/`vexpand` on children |
| `alignment: start/center/end` | `justify-content` | spacer views at the free ends | `halign`/`valign` on the box |
| `alignment: space-between` | `justify-content` | `distribution="equalSpacing"` | warning |
| `alignment: space-around/space-evenly` | `justify-content` | spacer views between and around, sized against each other | warning |
| `horizontal-stretch`, `vertical-stretch` | `flex-grow`, grid track `Nfr` | content hugging 250 (grows) or 750 (keeps size), plus proportional constraints between growing views | `hexpand`/`vexpand`; different ratios warn |
| `width`, `height` | `width`/`height` and `flex-basis` | width and height constraints | `width-request`, `height-request` |
| `min-*`, `max-*` | `min-*`/`max-*` | `>=` and `<=` constraints | `min-*` as a size request; `max-*` warns |
| `preferred-*` | `flex-basis` | constraint at priority 700 | warning |
| `x`, `y` outside layouts | `position:absolute` | leading and top constraints | `Gtk.Overlay` child with `halign`/`valign: start` and margins |

Slint's stretch rule is kept: when every child of a box has stretch 0, they
all grow. Elements outside a layout are centred when they have a size or
their widget keeps its own size. Otherwise they fill their parent.

Stack views cannot span grid cells. A `rowspan` or `colspan` above 1 takes
one slot in its row and warns.

## Properties

| Slint | HTML | UIKit | AppKit | GTK 4 |
| --- | --- | --- | --- | --- |
| `text` | element text | `text` / button title | cell `title` | `label` / `text` |
| `color` | `color` | `textColor` | cell `textColor` | Pango `foreground` |
| `font-size`, `font-weight`, `font-family`, `font-italic` | CSS font properties | `fontDescription` | `font` (no italic) | Pango markup |
| `horizontal-alignment` | `text-align` | `textAlignment` | cell `alignment` | `xalign` |
| `vertical-alignment` | flex `justify-content` | only `center` | only `top` | `yalign` |
| `wrap`, `overflow` | `white-space`, `text-overflow` | `numberOfLines`, `lineBreakMode` | cell `lineBreakMode` | `wrap`, `ellipsize` |
| `background` | `background` | `backgroundColor` | warning | warning |
| `visible`, `enabled`, `opacity` | `visibility:hidden`, `disabled`, `opacity` | `hidden`, `enabled`, `alpha` | `hidden`, cell `enabled`, `alphaValue` | `visible`, `sensitive`, `opacity` |
| `accessible-label` | `aria-label` | `accessibilityConfiguration` | warning | `accessibility { label }` |
| `checked` | `checked` | `on` | `state` | `active` |
| `minimum`, `maximum`, `value`, `step` | `min`, `max`, `value`, `step` | `minValue`/`maxValue`/`value` or stepper values | cell values | `Gtk.Adjustment` |
| `model`, `current-index` (`ComboBox`) | `<option selected>` | button menu with `menuElementState` | `NSPopUpButton` menu | `Gtk.StringList`, `selected` |
| `source`, `image-fit` (`Image`) | `src`, `object-fit` | `image`, `contentMode` | cell `image`, `imageScaling` | `file`, `content-fit` |

Lengths in `px`, `phx`, `pt`, `in`, `cm` and `mm` are converted to logical
pixels. Interface Builder points and GTK pixels are taken as equal to Slint
logical pixels. `rem` and `%` are kept in XML and HTML only. Colors are
`#` literals or CSS names; they are resolved to RGBA for UIKit, AppKit and
Pango.

## Bindings and handlers

Slint expressions are never evaluated. They are kept as text, together with
the names they read.

| Slint | XML | HTML | Storyboard | Blueprint |
| --- | --- | --- | --- | --- |
| `prop: expr` | `<property expr=… references=…>` | `data-bind-prop="expr"` | outlet to the view, comment `binding prop : expr` | comment `// binding prop : expr` |
| `prop <=> target` | `kind="two-way"` | `data-sync-prop` | outlet, comment `binding prop <=> target` | comment |
| `callback => { … }` | `<handler>` | `data-on-callback` | action `ownerCallback:` to the controller, the body as a comment | signal handler `$on_owner_callback()` |
| `changed prop => { … }` | `<handler kind="changed">` | `data-changed-prop` | outlet and comment; warns, needs code | `notify::prop` handler |
| `for x[i] in model:` | `<for>` | `data-for`, `data-for-item`, `data-for-index` | comment and outlet; the template is emitted once | comment; template emitted once |
| `if cond:` | `<if>` | `data-if` | comment and outlet | comment |
| component properties and callbacks | `<declare>`, `<callback>` | `data-prop-*`, `data-callbacks` | comment on the view controller | comment above the window |

Storyboard actions use the control's event: `touchUpInside` for buttons,
`valueChanged` for switches, sliders and steppers, `editingChanged` and
`primaryActionTriggered` for text fields. A `TouchArea` with `clicked`
becomes a tap gesture recognizer on UIKit. An AppKit control sends one
action, so a second callback on the same control warns.

## Not converted

These are kept as a comment and reported as a warning:

- `animate`, `states` and transitions;
- functions, and callbacks declared on inner elements;
- unknown elements and elements imported from other files;
- drawing properties such as `border-*` and `drop-shadow-*` in Storyboard
  and Blueprint (HTML keeps the ones CSS has);
- AppKit: backgrounds, italic fonts, `accessible-label`, scrolling, and
  `TouchArea` events;
- UIKit: indeterminate progress, and label vertical alignment other than
  `center`.
