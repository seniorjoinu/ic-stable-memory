# Benchmarks in real canister using performance counter API
[Here](../examples/performance_counter) is the canister. Run it, open the Candid UI interface and use it
to check each method yourself.

## `Vec` vs `SVec` vs `SLog`

### Push `100_000` elements
```
vec -> `6475564`
svec -> `26655890` - x4.1 slower
slog -> `14153495` - x2.2 slower
```

### Get `100_000` elements
```
vec -> `2700206`
svec -> `17200206` - x6.4 slower
slog -> `34324270` - x12.7 slower
```

### Pop `100_000` elements
```
vec -> `1800205`
svec -> `10800205` - x6 slower
slog -> `11356247` - x6.3 slower
```

### Insert `10_000` elements at index `0`
```
vec -> `1501540165`
svec -> `1810841649` - x1.2 slower
```

### Remove `10_000` elements from index `0`
```
vec -> `1501320269`
svec -> `1808971703` - x1.2 slower
```

## `HashMap` vs `SHashMap`

### Insert `100_000` entries
```
hashmap -> `122284638`
shashmap -> `214530518` - x1.75 slower
```

### Get `100_000` entries
```
hashmap -> `47729801`
shashmap -> `48427466` - x1.01 slower
```

### Remove `100_000` entries
```
hashmap -> `56731751`
shashmap -> `97830942` - x1.72 slower
```

## `HashSet` vs `SHashSet`

### Insert `100_000` keys
```
hashset -> `123675757`
shashset -> `188571411` - x1.53 slower
```

### Contains `100_000` keys
```
hashset -> `53030507`
shashset -> `42327466` - x1.25 faster
```

### Remove `100_000` keys
```
hashset -> `56752000`
shashset -> `94205462` - x1.66 slower
```

## `BTreeMap` vs `SBTreeMap`

### Insert `100_000` entries
```
btreemap -> `201187638`
sbtreemap -> `424801097` - x2.1 slower
```

### Get `100_000` entries
```
btreemap -> `86267536`
sbtreemap -> `275698231` - x3.2 slower
```

### Remove `100_000` entries
```
btreemap -> `157682120`
sbtreemap -> `501973804` - x3.2 slower
```

## `BTreeSet` vs `SBTreeSet`

### Insert `100_000` keys
```
btreeset -> `190744590`
sbtreeset -> `477202531` - x2.5 slower
```

### Contains `100_000` keys
```
btreeset -> `84467536`
sbtreeset -> `267576134` - x3.2 slower
```

### Remove `100_000` keys
```
btreeset -> `112840917`
sbtreeset -> `617281117` - x5.5 slower
```

## `RBTree` vs `SCertifiedBTreeMap`

### Insert `5_000` entries
```
rbtree -> `5627092211`
scertifiedbtreemap -> `9108725043` - x1.6 slower
scertifiedbtreemap (in batches of 10) -> `1354608056` - x4.1 faster
```

### Witness `5_000` entries
```
rbtree -> `3273570622`
scertifiedbtreemap -> `3541619761` - x1.08 slower
```

### Remove `5_000` entries
```
rbtree -> `9359364040`
scertifiedbtreemap -> `6693095737` - x1.4 faster
scertifiedbtreemap (in batches of 10) -> `731156025` - x12.8 faster
```