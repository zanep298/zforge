# Skill: ios-testing

## Purpose
Enforce TDD-first iOS testing using XCTest and Swift Testing.
Read this before writing any test.
Test command: `{{test_command}}`

## Framework Choice

| Use | When |
|-----|------|
| `Swift Testing` (`@Test`, `#expect`) | New code, iOS 16+ targets |
| `XCTest` | Legacy code, UI tests, snapshot tests |

## Structure

```
Sources/
  Features/Login/
    LoginViewModel.swift
Tests/
  Features/Login/
    LoginViewModelTests.swift   ← unit, mirrors source structure
  Integration/
    AuthFlowTests.swift
UITests/
  LoginUITests.swift
```

Mirror the source tree in the test target. One test file per source file.

## Unit Test Pattern (Swift Testing)

```swift
import Testing
@testable import MyApp

@Suite("LoginViewModel")
struct LoginViewModelTests {

    @Test("shows error when credentials are wrong")
    func showsErrorOnInvalidCredentials() async {
        // Arrange
        let auth = MockAuthUseCase(result: .failure(APIError.unauthorized))
        let vm = LoginViewModel(authUseCase: auth)
        vm.email = "user@example.com"
        vm.password = "wrong"

        // Act
        await vm.login()

        // Assert
        #expect(vm.errorMessage != nil)
        #expect(vm.isLoading == false)
    }

    @Test("clears error on new login attempt")
    func clearsErrorOnRetry() async {
        let auth = MockAuthUseCase(result: .failure(APIError.unauthorized))
        let vm = LoginViewModel(authUseCase: auth)
        await vm.login()
        #expect(vm.errorMessage != nil)

        auth.result = .success(User.fixture)
        await vm.login()
        #expect(vm.errorMessage == nil)
    }
}
```

## Unit Test Pattern (XCTest)

```swift
final class LoginViewModelTests: XCTestCase {

    func test_login_setsErrorMessage_whenCredentialsInvalid() async {
        // Arrange
        let auth = MockAuthUseCase(result: .failure(APIError.unauthorized))
        let sut = LoginViewModel(authUseCase: auth)

        // Act
        await sut.login()

        // Assert
        XCTAssertNotNil(sut.errorMessage)
        XCTAssertFalse(sut.isLoading)
    }
}
```

## Naming

```swift
// good — describes scenario and expected outcome
test_fetchUser_returnsUser_whenCacheIsWarm()
showsLoadingIndicator_whileRequestInFlight()
rejectsEmptyEmail_beforeSubmit()

// bad
testLogin()
test1()
testViewModel()
```

## Mocking with Protocols

```swift
// Define mock in the test target — never ship mocks in production
final class MockAuthUseCase: AuthUseCase {
    var result: Result<User, Error>
    private(set) var loginCallCount = 0

    init(result: Result<User, Error>) {
        self.result = result
    }

    func login(email: String, password: String) async throws -> User {
        loginCallCount += 1
        return try result.get()
    }
}
```

Mocks track call counts and capture arguments — verify interactions, not just state.

## Test Fixtures

```swift
extension User {
    static let fixture = User(
        id: UUID(uuidString: "00000000-0000-0000-0000-000000000001")!,
        email: "fixture@example.com",
        displayName: "Fixture User"
    )
}
```

Define fixtures in `Tests/Support/Fixtures/` — never in production code.

## Async Testing

```swift
// good — await directly, Swift Testing handles async natively
@Test func loadsProfileOnAppear() async {
    let vm = ProfileViewModel(repo: MockProfileRepo())
    await vm.loadProfile()
    #expect(vm.profile != nil)
}

// XCTest — use async test method
func test_loadsProfile() async throws {
    let vm = ProfileViewModel(repo: MockProfileRepo())
    await vm.loadProfile()
    XCTAssertNotNil(vm.profile)
}
```

Never use `XCTestExpectation` for `async/await` code — it is for callbacks only.

## Test-First Rule

1. Write the failing test — run `{{test_command}}` and confirm it fails (red)
2. Write the minimal production code to make it pass (green)
3. Run `{{test_command}}` again — confirm it passes
4. Refactor — confirm tests still pass

Never write a ViewModel, UseCase, or Repository method before its failing test exists.

## Coverage Targets

- ViewModels: 90%+ (pure logic, easy to test)
- UseCases: 90%+ (no UI dependencies)
- Repositories: 80%+ (integration tests via in-memory fakes)
- Views: snapshot tests for key states (loading, error, empty, populated)

## Do Not Do

- No `sleep` or `DispatchQueue.main.asyncAfter` in tests — use `await` or `confirmation`
- No `XCTestExpectation` for `async/await` code
- No testing SwiftUI `View` body logic — move logic to ViewModel and test there
- No `@testable import` for things that should be `public` — fix the visibility instead
- No skipping tests that are hard to write — redesign the production code for testability
