# Skill: ios-patterns

## Purpose
Enforce idiomatic Swift and clean architecture in this iOS project.
Read this before implementing any iOS task.

## Architecture: MVVM + Clean Layers

```
View (SwiftUI)          ← renders state, sends user actions
  └── ViewModel         ← @Observable or ObservableObject, owns UI state
        └── UseCase     ← single-responsibility business logic
              └── Repository (protocol) ← abstracts data source
                    └── Impl (network / local / mock)
```

- Views own no business logic — only layout and user intent forwarding
- ViewModels own no networking — delegate to use cases
- Repositories are protocols; concrete types are injected

## Swift Idioms

### Value types first
```swift
// good — struct, copied not shared
struct UserProfile {
    let id: UUID
    var displayName: String
}

// bad — class when value semantics suffice
class UserProfile { ... }
```

### Result and async/await
```swift
// good — structured concurrency
func fetchUser(id: UUID) async throws -> User

// good — Result for synchronous operations with explicit errors
func parse(_ data: Data) -> Result<User, ParseError>

// bad — completion handlers in new code
func fetchUser(id: UUID, completion: @escaping (User?, Error?) -> Void)
```

### Errors as enums
```swift
enum APIError: LocalizedError {
    case unauthorized
    case notFound(id: UUID)
    case decodingFailed(underlying: Error)

    var errorDescription: String? {
        switch self {
        case .unauthorized: return "Session expired. Please log in again."
        case .notFound(let id): return "Resource \(id) not found."
        case .decodingFailed: return "Unexpected server response."
        }
    }
}
```

### Protocol-based injection
```swift
protocol UserRepository {
    func user(id: UUID) async throws -> User
    func save(_ user: User) async throws
}

// production
final class RemoteUserRepository: UserRepository { ... }

// tests
final class MockUserRepository: UserRepository { ... }
```

## State Management

- `@Observable` (iOS 17+) or `ObservableObject` + `@Published` for ViewModels
- `@State` for local, ephemeral UI state only (text field value, sheet toggle)
- `@EnvironmentObject` / `@Environment` for app-wide dependencies (auth, theme)
- Never reach up from a child view to mutate parent state — pass bindings or callbacks

```swift
@Observable
final class LoginViewModel {
    var email = ""
    var password = ""
    var isLoading = false
    private(set) var errorMessage: String?

    private let authUseCase: AuthUseCase

    init(authUseCase: AuthUseCase) {
        self.authUseCase = authUseCase
    }

    func login() async {
        isLoading = true
        defer { isLoading = false }
        do {
            try await authUseCase.login(email: email, password: password)
        } catch {
            errorMessage = error.localizedDescription
        }
    }
}
```

## Dependency Injection

- Inject via initializer — no singletons in business logic
- Use a `DependencyContainer` or environment key at the app root
- Never access `UserDefaults`, `URLSession`, or `CoreData` directly from a ViewModel

## Module Organization

```
Features/
  Login/
    LoginView.swift
    LoginViewModel.swift
    LoginUseCase.swift
  Profile/
    ProfileView.swift
    ProfileViewModel.swift
Domain/
  Models/
    User.swift
  Repositories/
    UserRepository.swift       ← protocol
Data/
  Remote/
    RemoteUserRepository.swift
  Local/
    LocalUserRepository.swift
```

Organize by feature, not by type. Keep feature folders cohesive.

## Concurrency

- Use `async/await` for all new async code — no Combine for networking in new code
- Mark `@MainActor` on ViewModels and any type that updates UI state
- Wrap callbacks in `withCheckedThrowingContinuation` when bridging legacy APIs
- Never `Task.detached` without a clear reason — prefer structured `Task { }` inside `@MainActor`

## Do Not Do

- No business logic in SwiftUI `View` bodies
- No force unwrap (`!`) in production paths — use `guard let` or `if let`
- No `DispatchQueue.main.async` when `@MainActor` suffices
- No massive ViewModels — split use cases when a ViewModel exceeds ~150 lines
- No direct `URLSession` calls outside the data layer
