# Skill: android-testing

## Purpose
Write reliable, idiomatic Kotlin tests for Android/KMP projects using Kotest and MockK.
Read this before writing any test for this project.

## TDD Workflow

```
RED     → write a failing test first
GREEN   → write minimal code to pass
REFACTOR → improve while keeping tests green
REPEAT  → next requirement
```

Always: write the test before the implementation.

## Test Setup (Gradle)

```kotlin
// build.gradle.kts (module)
dependencies {
    testImplementation("io.kotest:kotest-runner-junit5:6.1.4")
    testImplementation("io.kotest:kotest-assertions-core:6.1.4")
    testImplementation("io.kotest:kotest-property:6.1.4")
    testImplementation("io.mockk:mockk:1.14.9")
    testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.10.2")
    testImplementation("app.cash.turbine:turbine:1.2.0")

    // ViewModel testing
    testImplementation("androidx.arch.core:core-testing:2.2.0")
    testImplementation("androidx.test:core-ktx:1.6.1")
}

tasks.withType<Test> { useJUnitPlatform() }
```

## UseCase Tests (Kotest FunSpec)

```kotlin
class GetItemsByCategoryUseCaseTest : FunSpec({
    val repository = mockk<ItemRepository>()
    val useCase = GetItemsByCategoryUseCase(repository)

    test("returns items for given category") {
        val items = listOf(Item("1", "Widget", "tools"))
        coEvery { repository.getItemsByCategory("tools") } returns Result.success(items)

        val result = useCase("tools")

        result.shouldBeSuccess()
        result.getOrThrow() shouldBe items
    }

    test("propagates repository failure") {
        coEvery { repository.getItemsByCategory(any()) } returns
            Result.failure(RuntimeException("DB error"))

        val result = useCase("tools")

        result.shouldBeFailure()
    }
})
```

## Repository Tests

```kotlin
class ItemRepositoryImplTest : FunSpec({
    val local = mockk<ItemLocalDataSource>()
    val remote = mockk<ItemRemoteDataSource>()
    val repo = ItemRepositoryImpl(local, remote)

    test("fetches from remote and caches locally") {
        val dtos = listOf(ItemDto("1", "Widget", "tools"))
        val entities = listOf(ItemEntity("1", "Widget", "tools"))
        coEvery { remote.fetchItems("tools") } returns dtos
        coEvery { local.insertItems(any()) } just Runs
        coEvery { local.getByCategory("tools") } returns entities

        val result = repo.getItemsByCategory("tools")

        result.shouldBeSuccess()
        coVerify { local.insertItems(entities) }
    }
})
```

## ViewModel Tests (Turbine for Flow)

```kotlin
class ItemListViewModelTest : FunSpec({
    val getItems = mockk<GetItemsByCategoryUseCase>()
    val observeItems = mockk<ObserveItemsUseCase>()

    beforeTest { Dispatchers.setMain(UnconfinedTestDispatcher()) }
    afterTest { Dispatchers.resetMain() }

    test("loads items for category") {
        val items = listOf(Item("1", "Widget", "tools"))
        coEvery { getItems("tools") } returns Result.success(items)
        every { observeItems() } returns flowOf(items)

        val vm = ItemListViewModel(getItems, observeItems)

        vm.state.test {
            vm.loadCategory("tools")
            val state = awaitItem()
            state.items shouldBe items
            state.isLoading shouldBe false
        }
    }

    test("sets error on failure") {
        coEvery { getItems(any()) } returns Result.failure(RuntimeException("oops"))
        every { observeItems() } returns emptyFlow()

        val vm = ItemListViewModel(getItems, observeItems)
        vm.loadCategory("tools")

        vm.state.value.error shouldNotBe null
        vm.state.value.isLoading shouldBe false
    }
})
```

## MockK Cheatsheet

```kotlin
// Basic mock
val repo = mockk<ItemRepository>()

// Suspend mock
coEvery { repo.getItemsByCategory("tools") } returns Result.success(items)

// Flow mock
every { repo.observeItems() } returns flowOf(items)

// Void mock
coEvery { repo.insertItems(any()) } just Runs

// Argument capture
val slot = slot<String>()
coEvery { repo.getItemsByCategory(capture(slot)) } returns Result.success(emptyList())
// then: slot.captured == actual argument

// Verify
coVerify(exactly = 1) { repo.getItemsByCategory("tools") }
verify { repo.observeItems() }
confirmVerified(repo)
```

## Kotest Assertions

```kotlin
result.shouldBeSuccess()
result.shouldBeFailure()
result.getOrThrow() shouldBe expectedValue

items shouldHaveSize 3
items.shouldContain(item)
items.shouldBeEmpty()

state.isLoading shouldBe false
state.error shouldNotBe null
```

## Testing Flows (Turbine)

```kotlin
repo.observeItems().test {
    val first = awaitItem()
    first shouldHaveSize 2
    cancelAndIgnoreRemainingEvents()
}
```

## Coroutine Test Setup

```kotlin
// Use UnconfinedTestDispatcher for ViewModels
beforeTest { Dispatchers.setMain(UnconfinedTestDispatcher()) }
afterTest { Dispatchers.resetMain() }

// Use runTest for suspend tests
test("suspend test") {
    runTest {
        val result = suspendFn()
        result shouldBe expected
    }
}
```

## Coverage

```bash
./gradlew koverHtmlReport       # HTML report → build/reports/kover/
./gradlew koverXmlReport        # XML for CI
./gradlew test                  # Run all tests
```

Target: 80%+ line coverage on domain and data layers.

## Naming Convention

Descriptive test names that explain the scenario:
- `returns items for given category`
- `propagates repository failure`
- `sets error state on network timeout`
- `does not emit when flow is empty`
