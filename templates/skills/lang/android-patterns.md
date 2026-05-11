# Skill: android-patterns

## Purpose
Enforce Clean Architecture and idiomatic Kotlin in this Android/KMP project.
Read this before implementing any Android task.

## Architecture: Clean Layers

```
app/                  ← DI wiring, Application class, entry point
presentation/         ← Screens, ViewModels, UI models, navigation
  └── ViewModel       ← viewModelScope, StateFlow, UI state
        └── UseCase   ← single-responsibility business operation
              └── Repository (interface) ← defined in domain
data/                 ← Repository implementations, DataSources, DB, network
domain/               ← Pure Kotlin: UseCases, domain models, interfaces
core/                 ← Shared utilities, base classes, error types
```

Dependency rule: `domain` NEVER imports Android, Room, Ktor, or any framework — pure Kotlin only.

## UseCase Pattern

```kotlin
class GetItemsByCategoryUseCase(private val repository: ItemRepository) {
    suspend operator fun invoke(category: String): Result<List<Item>> =
        repository.getItemsByCategory(category)
}

class ObserveItemsUseCase(private val repository: ItemRepository) {
    operator fun invoke(): Flow<List<Item>> = repository.observeItems()
}
```

## Repository Pattern

Interface in domain, implementation in data:

```kotlin
// domain — interface only
interface ItemRepository {
    suspend fun getItemsByCategory(category: String): Result<List<Item>>
    fun observeItems(): Flow<List<Item>>
}

// data — implementation
class ItemRepositoryImpl(
    private val local: ItemLocalDataSource,
    private val remote: ItemRemoteDataSource,
) : ItemRepository {
    override suspend fun getItemsByCategory(category: String): Result<List<Item>> =
        runCatching {
            val items = remote.fetchItems(category)
            local.insertItems(items.map { it.toEntity() })
            local.getByCategory(category).map { it.toDomain() }
        }

    override fun observeItems(): Flow<List<Item>> =
        local.observeAll().map { entities -> entities.map { it.toDomain() } }
}
```

## ViewModel Pattern

```kotlin
@HiltViewModel
class ItemListViewModel @Inject constructor(
    private val getItems: GetItemsByCategoryUseCase,
    private val observeItems: ObserveItemsUseCase,
) : ViewModel() {

    private val _state = MutableStateFlow(ItemListState())
    val state: StateFlow<ItemListState> = _state.asStateFlow()

    init {
        observeItems()
            .onEach { items -> _state.update { it.copy(items = items) } }
            .launchIn(viewModelScope)
    }

    fun loadCategory(category: String) {
        viewModelScope.launch {
            _state.update { it.copy(isLoading = true) }
            getItems(category)
                .onSuccess { items -> _state.update { it.copy(items = items, isLoading = false) } }
                .onFailure { e -> _state.update { it.copy(error = e.message, isLoading = false) } }
        }
    }
}

data class ItemListState(
    val items: List<Item> = emptyList(),
    val isLoading: Boolean = false,
    val error: String? = null,
)
```

## Kotlin Idioms

### Null safety — no `!!`
```kotlin
// good
val email = user?.email ?: "unknown@example.com"
// bad
val email = user!!.email
```

### Immutability — `val` over `var`, `copy()` for updates
```kotlin
data class User(val id: String, val name: String, val email: String)
fun updateEmail(user: User, new: String): User = user.copy(email = new)
```

### Sealed classes for exhaustive results
```kotlin
sealed interface AppError {
    data class Network(val message: String) : AppError
    data class Database(val message: String) : AppError
    data object Unauthorized : AppError
}
```

### Coroutines — always structured
```kotlin
// good — scoped
viewModelScope.launch { fetchData() }
coroutineScope { val a = async { fetchA() }; val b = async { fetchB() } }

// bad — unstructured
GlobalScope.launch { fetchData() }
```

### CancellationException — always rethrow
```kotlin
try { suspendWork() }
catch (e: CancellationException) { throw e }  // preserve cancellation
catch (e: Exception) { handleError(e) }
```

## Mappers

Keep as extension functions near data models:

```kotlin
fun ItemEntity.toDomain() = Item(id = id, title = title, category = category)
fun ItemDto.toEntity() = ItemEntity(id = id, title = title, category = category)
```

## Dependency Injection

### Hilt (Android-only)
```kotlin
@Module @InstallIn(SingletonComponent::class)
abstract class RepositoryModule {
    @Binds abstract fun bindItemRepo(impl: ItemRepositoryImpl): ItemRepository
}
```

### Koin (KMP-friendly)
```kotlin
val dataModule = module {
    single<ItemRepository> { ItemRepositoryImpl(get(), get()) }
}
val domainModule = module {
    factory { GetItemsByCategoryUseCase(get()) }
}
```

## Room (Android)

```kotlin
@Entity(tableName = "items")
data class ItemEntity(@PrimaryKey val id: String, val title: String, val category: String)

@Dao
interface ItemDao {
    @Query("SELECT * FROM items WHERE category = :cat")
    suspend fun getByCategory(cat: String): List<ItemEntity>

    @Upsert suspend fun upsert(items: List<ItemEntity>)

    @Query("SELECT * FROM items")
    fun observeAll(): Flow<List<ItemEntity>>
}
```

## Compose Checklist

- Use `collectAsStateWithLifecycle()` not `collectAsState()`
- Collect Flow in UI via `repeatOnLifecycle(Lifecycle.State.STARTED)`
- Pass lambdas to composables, not `NavController`
- Add `key()` in `LazyColumn` items for stable identity
- Avoid lambda allocations in parameters — use `remember(key) { lambda }`

## Anti-Patterns

- Framework imports in `domain` module (Android, Room, Ktor) — violation
- Exposing `Entity` or `Dto` to the UI layer — always map to domain models
- Business logic in ViewModel — extract to UseCase
- `GlobalScope` usage — always use structured scopes
- Storing `Activity`/`Fragment` reference in ViewModel — causes leaks
- Mutable collections inside StateFlow — use `copy()` and immutable lists
