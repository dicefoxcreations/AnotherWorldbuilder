/**
 * What is worked out from the world's lists, kept as long as the lists are.
 *
 * The documents and types come as arrays that are replaced, never changed,
 * when something loads. So whatever follows from them — an index by name,
 * the documents of a type in order — is worked out once per array, not once
 * per keystroke. With thousands of documents that is what keeps typing fast.
 */
export function perList<A extends object, R>(make: (a: A) => R): (a: A) => R {
  const kept = new WeakMap<A, R>();
  return (a) => {
    if (!kept.has(a)) kept.set(a, make(a));
    return kept.get(a)!;
  };
}

/** The same for two lists — the documents and the types. */
export function perLists<A extends object, B extends object, R>(make: (a: A, b: B) => R): (a: A, b: B) => R {
  const kept = new WeakMap<A, WeakMap<B, R>>();
  return (a, b) => {
    let inner = kept.get(a);
    if (!inner) kept.set(a, (inner = new WeakMap()));
    if (!inner.has(b)) inner.set(b, make(a, b));
    return inner.get(b)!;
  };
}
