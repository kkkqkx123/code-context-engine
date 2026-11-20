//! Single source of truth for the metadata storage contract.
//!
//! The declaration below generates the backend-neutral contract, its
//! per-domain views, and the deterministic backend forwarding impls. Adding
//! or removing a method only requires editing the declaration once.

/// Expand one declaration into the contract traits and backend forwarding.
///
/// Each domain becomes its own module with a narrow trait. Every concrete
/// backend keeps one `MetadataStorage` implementation; the narrow traits are
/// provided uniformly from that implementation so callers still depend on a
/// single composed contract. The `Arc` and `MetadataBackendStore` impls are
/// generated with the same method list, keeping forwarding in lockstep.
macro_rules! metadata_storage_contract {
    (
        $(
            $module:ident : $trait_name:ident {
                $(
                    $(#[$attr:meta])*
                    fn $method:ident(&self $(, $arg:ident : $arg_ty:ty)* $(,)?) -> $ret:ty;
                )*
            }
        )+
    ) => {
        $(
            pub mod $module {
                use super::*;

                pub trait $trait_name: Clone + Send + Sync + 'static {
                    $(
                        $(#[$attr])*
                        fn $method(
                            &self $(, $arg: $arg_ty)*
                        ) -> impl Future<Output = $ret> + Send;
                    )*
                }

                impl<T: MetadataStorage> $trait_name for T {
                    $(
                        fn $method(
                            &self $(, $arg: $arg_ty)*
                        ) -> impl Future<Output = $ret> + Send {
                            MetadataStorage::$method(self $(, $arg)*)
                        }
                    )*
                }
            }

            pub use $module::$trait_name;
        )+

        pub trait MetadataStorage: Clone + Send + Sync + 'static {
            $(
                $(
                    $(#[$attr])*
                    fn $method(
                        &self $(, $arg: $arg_ty)*
                    ) -> impl Future<Output = $ret> + Send;
                )*
            )+

            /// Backend name for logging.
            fn backend_name(&self) -> &'static str {
                "local"
            }

            /// Whether this branch keeps per-project database files.
            fn is_per_project_db(&self) -> bool {
                true
            }
        }

        impl<T: MetadataStorage> MetadataStorage for std::sync::Arc<T> {
            $(
                $(
                    fn $method(
                        &self $(, $arg: $arg_ty)*
                    ) -> impl Future<Output = $ret> + Send {
                        T::$method(self.as_ref() $(, $arg)*)
                    }
                )+
            )+

            fn backend_name(&self) -> &'static str {
                T::backend_name(self.as_ref())
            }

            fn is_per_project_db(&self) -> bool {
                T::is_per_project_db(self.as_ref())
            }
        }

        impl<L, R> MetadataStorage for MetadataBackendStore<L, R>
        where
            L: MetadataStorage,
            R: MetadataStorage,
        {
            $(
                $(
                    fn $method(
                        &self $(, $arg: $arg_ty)*
                    ) -> impl Future<Output = $ret> + Send {
                        async move {
                            match self {
                                Self::Local(store) => L::$method(store $(, $arg)*).await,
                                Self::Remote(store) => R::$method(store $(, $arg)*).await,
                            }
                        }
                    }
                )+
            )+

            fn backend_name(&self) -> &'static str {
                match self {
                    Self::Local(store) => L::backend_name(store),
                    Self::Remote(store) => R::backend_name(store),
                }
            }

            fn is_per_project_db(&self) -> bool {
                match self {
                    Self::Local(store) => L::is_per_project_db(store),
                    Self::Remote(store) => R::is_per_project_db(store),
                }
            }
        }
    };
}
