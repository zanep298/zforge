# Skill: ios-ui-patterns

## Purpose
Guide SwiftUI implementation from Figma designs.
Read this alongside `figma.md` (if present) before implementing any UI task.

## Figma → SwiftUI Mapping

| Figma concept | SwiftUI equivalent |
|---------------|--------------------|
| Frame (auto-layout, vertical) | `VStack` |
| Frame (auto-layout, horizontal) | `HStack` |
| Frame (auto-layout, wrap) | `LazyVGrid` / `FlowLayout` |
| Component / instance | SwiftUI `View` struct |
| Component set (variants) | View with enum parameter |
| Design token (color) | `Color` extension or `ShapeStyle` |
| Design token (typography) | `Font` extension |
| Design token (spacing) | `CGFloat` constants |
| Overlay | `ZStack` or `.overlay` modifier |
| Clip mask / corner radius | `.clipShape` / `.cornerRadius` |

## Design Token Extraction

Map Figma tokens to Swift constants before writing views.
Define tokens in a `DesignSystem` namespace:

```swift
extension Color {
    static let surfacePrimary   = Color("surface/primary")    // Assets catalog
    static let textPrimary      = Color("text/primary")
    static let accentBlue       = Color("accent/blue")
}

extension Font {
    static let headingLarge  = Font.custom("Inter-Bold", size: 28)
    static let bodyRegular   = Font.custom("Inter-Regular", size: 16)
    static let labelSmall    = Font.custom("Inter-Medium", size: 12)
}

enum Spacing {
    static let xs: CGFloat = 4
    static let sm: CGFloat = 8
    static let md: CGFloat = 16
    static let lg: CGFloat = 24
    static let xl: CGFloat = 40
}
```

Never hardcode hex values or magic numbers in view files.

## Component Structure

Each Figma component becomes a focused SwiftUI struct:

```swift
// good — one component per file, named after Figma component
struct PrimaryButton: View {
    let title: String
    let action: () -> Void
    var isLoading: Bool = false

    var body: some View {
        Button(action: action) {
            ZStack {
                if isLoading {
                    ProgressView().tint(.white)
                } else {
                    Text(title)
                        .font(.labelSmall)
                        .foregroundStyle(Color.white)
                }
            }
            .frame(maxWidth: .infinity)
            .frame(height: 52)
            .background(Color.accentBlue)
            .clipShape(RoundedRectangle(cornerRadius: 12))
        }
        .disabled(isLoading)
    }
}
```

## Figma Variant → Swift Enum

```swift
// Figma component set "Button" with variants: style=primary|secondary|destructive
struct AppButton: View {
    enum Style { case primary, secondary, destructive }

    let title: String
    let style: Style
    let action: () -> Void

    var backgroundColor: Color {
        switch style {
        case .primary:     return .accentBlue
        case .secondary:   return .surfacePrimary
        case .destructive: return .red
        }
    }
}
```

## Layout from Figma Auto-Layout

Read Figma's auto-layout panel directly:

```
Figma: Direction=Vertical, Gap=16, Padding=24, Alignment=leading
→
VStack(alignment: .leading, spacing: Spacing.md) {
    ...
}
.padding(Spacing.lg)
```

```
Figma: Direction=Horizontal, Gap=8, Alignment=center
→
HStack(spacing: Spacing.sm) {
    ...
}
```

## State Variants

Implement all Figma states, not just the default:

| Figma state | SwiftUI approach |
|-------------|-----------------|
| Default | base view |
| Hover / Pressed | `.buttonStyle` / `.scaleEffect` on tap |
| Loading | conditional `ProgressView` overlay |
| Error | conditional error banner / red border |
| Empty | `ContentUnavailableView` or custom empty state |
| Disabled | `.disabled(true)` + reduced opacity |

```swift
struct InputField: View {
    @Binding var text: String
    var error: String? = nil

    var body: some View {
        VStack(alignment: .leading, spacing: Spacing.xs) {
            TextField("", text: $text)
                .padding(Spacing.md)
                .background(Color.surfacePrimary)
                .overlay(
                    RoundedRectangle(cornerRadius: 8)
                        .strokeBorder(error != nil ? Color.red : Color.clear, lineWidth: 1)
                )
            if let error {
                Text(error)
                    .font(.labelSmall)
                    .foregroundStyle(Color.red)
            }
        }
    }
}
```

## Accessibility

Every interactive element must have:

```swift
Button(action: dismiss) {
    Image(systemName: "xmark")
}
.accessibilityLabel("Close")
.accessibilityHint("Dismisses this sheet")
```

- Decorative images: `.accessibilityHidden(true)`
- Dynamic type: use `.font(.body)` semantic sizes, not fixed-size custom fonts alone
- Minimum tap target: 44×44pt — use `.contentShape(Rectangle())` to expand hit area

## Preview with Design Tokens

```swift
#Preview {
    VStack(spacing: Spacing.md) {
        PrimaryButton(title: "Continue", action: {})
        PrimaryButton(title: "Loading...", action: {}, isLoading: true)
    }
    .padding(Spacing.lg)
    .background(Color.surfacePrimary)
}
```

Always preview all states: default, loading, error, empty.

## Do Not Do

- No magic numbers for spacing, color, or font size — use design token constants
- No duplicating Figma component logic across views — extract a shared `View` struct
- No skipping error and empty states — implement all Figma-specified variants
- No `UIColor` or `UIFont` in SwiftUI code — use `Color` and `Font`
- No layout logic in ViewModels — keep all `body` code in the `View` layer
- No accessibility labels missing on interactive elements
