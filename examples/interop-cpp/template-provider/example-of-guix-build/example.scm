(use-modules 
  (guix search-paths)
  (guix store)
  (guix utils)
  (guix derivations)
  (guix packages)
  (guix build-system)
  (guix build-system gnu)
  (ice-9 match)
  (ice-9 vlist)
  (srfi srfi-1)
  (srfi srfi-26))

(define %crate-base-url
  (make-parameter "https://crates.io"))
(define crate-url
  (string-append (%crate-base-url) "/api/v1/crates/"))
(define crate-url?
  (cut string-prefix? crate-url <>))

(define (crate-uri name version)
  "Return a URI string for the crate package hosted at crates.io corresponding
to NAME and VERSION."
  (string-append crate-url name "/" version "/download"))

(define (default-rust)
  "Return the default Rust package."
  ;; Lazily resolve the binding to avoid a circular dependency.
  (let ((rust (resolve-interface '(gnu packages rust))))
    (module-ref rust 'rust-1.51)))


(define %cargo-utils-modules
  ;; Build-side modules imported by default.
  `((guix build cargo-utils)
    ,@%gnu-build-system-modules))

(define %cargo-build-system-modules
  ;; Build-side modules imported by default.
  `((guix build cargo-build-system)
    (guix build json)
    ,@%cargo-utils-modules))

(define* (cargo-build store name inputs
                      #:key
                      (tests? #t)
                      (test-target #f)
                      (vendor-dir "guix-vendor")
                      (cargo-build-flags ''("--release"))
                      (cargo-test-flags ''("--release"))
                      (cargo-package-flags ''("--no-metadata" "--no-verify"))
                      (features ''())
                      (skip-build? #f)
                      (install-source? #t)
                      (phases '(@ (guix build cargo-build-system)
                                  %standard-phases))
                      (outputs '("out"))
                      (search-paths '())
                      (system (%current-system))
                      (guile #f)
                      (imported-modules %cargo-build-system-modules)
                      (modules '((guix build cargo-build-system)
                                 (guix build utils))))
  "Build SOURCE using CARGO, and with INPUTS."

  (define builder
    `(begin
       (use-modules ,@modules)
       (cargo-build #:name ,name
                    #:source ,(match (assoc-ref inputs "source")
                                (((? derivation? source))
                                 (derivation->output-path source))
                                ((source)
                                 source)
                                (source
                                 source))
                    #:system ,system
                    #:test-target ,test-target
                    #:vendor-dir ,vendor-dir
                    #:cargo-build-flags ,cargo-build-flags
                    #:cargo-test-flags ,cargo-test-flags
                    #:cargo-package-flags ,cargo-package-flags
                    #:features ,features
                    #:skip-build? ,skip-build?
                    #:install-source? ,install-source?
                    #:tests? ,(and tests? (not skip-build?))
                    #:phases ,phases
                    #:outputs %outputs
                    #:search-paths ',(map search-path-specification->sexp
                                          search-paths)
                    #:inputs %build-inputs)))

  (define guile-for-build
    (match guile
      ((? package?)
       (package-derivation store guile system #:graft? #f))
      (#f                                         ; the default
       (let* ((distro (resolve-interface '(gnu packages commencement)))
              (guile  (module-ref distro 'guile-final)))
         (package-derivation store guile system #:graft? #f)))))

  (build-expression->derivation store name builder
                                #:inputs inputs
                                #:system system
                                #:modules imported-modules
                                #:outputs outputs
                                #:guile-for-build guile-for-build))

(define (package-cargo-inputs p)
  (apply
    (lambda* (#:key (cargo-inputs '()) #:allow-other-keys)
      cargo-inputs)
    (package-arguments p)))

(define (package-cargo-development-inputs p)
  (apply
    (lambda* (#:key (cargo-development-inputs '()) #:allow-other-keys)
      cargo-development-inputs)
    (package-arguments p)))

(define (crate-closure inputs)
  "Return the closure of INPUTS when considering the 'cargo-inputs' and
'cargod-dev-deps' edges.  Omit duplicate inputs, except for those
already present in INPUTS itself.

This is implemented as a breadth-first traversal such that INPUTS is
preserved, and only duplicate extracted inputs are removed.

Forked from ((guix packages) transitive-inputs) since this extraction
uses slightly different rules compared to the rest of Guix (i.e. we
do not extract the conventional inputs)."
  (define (seen? seen item)
    ;; FIXME: We're using pointer identity here, which is extremely sensitive
    ;; to memoization in package-producing procedures; see
    ;; <https://bugs.gnu.org/30155>.
    (vhash-assq item seen))

  (let loop ((inputs     inputs)
             (result     '())
             (propagated '())
             (first?     #t)
             (seen       vlist-null))
    (match inputs
      (()
       (if (null? propagated)
           (reverse result)
           (loop (reverse (concatenate propagated)) result '() #f seen)))
      (((and input (label (? package? package))) rest ...)
       (if (and (not first?) (seen? seen package))
           (loop rest result propagated first? seen)
           (loop rest
                 (cons input result)
                 (cons (package-cargo-inputs package)
                       propagated)
                 first?
                 (vhash-consq package package seen))))
      ((input rest ...)
       (loop rest (cons input result) propagated first? seen)))))

(define (expand-crate-sources cargo-inputs cargo-development-inputs)
  "Extract all transitive sources for CARGO-INPUTS and CARGO-DEVELOPMENT-INPUTS
along their 'cargo-inputs' edges.

Cargo requires all transitive crate dependencies' sources to be available
in its index, even if they are optional (this is so it can generate
deterministic Cargo.lock files regardless of the target platform or enabled
features). Thus we need all transitive crate dependencies for any cargo
dev-dependencies, but this is only needed when building/testing a crate directly
(i.e. we will never need transitive dev-dependencies for any dependency crates).

Another complication arises due potential dependency cycles from Guix's
perspective: Although cargo does not permit cyclic dependencies between crates,
however, it permits cycles to occur via dev-dependencies. For example, if crate
X depends on crate Y, crate Y's tests could pull in crate X to to verify
everything builds properly (this is a rare scenario, but it it happens for
example with the `proc-macro2` and `quote` crates). This is allowed by cargo
because tests are built as a pseudo-crate which happens to depend on the
X and Y crates, forming an acyclic graph.

We can side step this problem by only considering regular cargo dependencies
since they are guaranteed to not have cycles. We can further resolve any
potential dev-dependency cycles by extracting package sources (which never have
any dependencies and thus no cycles can exist).

There are several implications of this decision:
* Building a package definition does not require actually building/checking
any dependent crates. This can be a benefits:
 - For example, sometimes a crate may have an optional dependency on some OS
 specific package which cannot be built or run on the current system. This
 approach means that the build will not fail if cargo ends up internally ignoring
 the dependency.
 - It avoids waiting for quadratic builds from source: cargo always builds
 dependencies within the current workspace. This is largely due to Rust not
 having a stable ABI and other resolutions that cargo applies. This means that
 if we have a depencency chain of X -> Y -> Z and we build each definition
 independently the following will happen:
  * Cargo will build and test crate Z
  * Cargo will build crate Z in Y's workspace, then build and test Y
  * Cargo will build crates Y and Z in X's workspace, then build and test X
* But there are also some downsides with this approach:
  - If a dependent crate is subtly broken on the system (i.e. it builds but its
  tests fail) the consuming crates may build and test successfully but
  actually fail during normal usage (however, the CI will still build all
  packages which will give visibility in case packages suddenly break).
  - Because crates aren't declared as regular inputs, other Guix facilities
  such as tracking package graphs may not work by default (however, this is
  something that can always be extended or reworked in the future)."
  (filter-map
    (match-lambda
      ((label (? package? p))
       (list label (package-source p)))
      ((label input)
       (list label input)))
    (crate-closure (append cargo-inputs cargo-development-inputs))))

(define* (lower name
                #:key source inputs native-inputs outputs system target
                (rust (default-rust))
                (cargo-inputs '())
                (cargo-development-inputs '())
                #:allow-other-keys
                #:rest arguments)
  "Return a bag for NAME."

  (define private-keywords
    '(#:source #:target #:rust #:inputs #:native-inputs #:outputs
      #:cargo-inputs #:cargo-development-inputs))

  (and (not target)
       (bag
         (name name)
         (system system)
         (target target)
         (host-inputs `(,@(if source
                              `(("source" ,source))
                              '())
                        ,@inputs

                        ;; Keep the standard inputs of 'gnu-build-system'
                        ,@(standard-packages)))
         (build-inputs `(("cargo" ,rust "cargo")
                         ("rustc" ,rust)
                         ,@(expand-crate-sources cargo-inputs cargo-development-inputs)
                         ,@native-inputs))
         (outputs outputs)
         (build cargo-build)
         (arguments (strip-keyword-arguments private-keywords arguments)))))

(define cargo-build-system
  (build-system
    (name 'cargo)
    (description
     "Cargo build system, to build Rust crates")
    (lower lower)))


(use-modules (gnu)
  (guix licenses)
  ;(guix build-system cargo2)
  (guix build-system copy)
  (guix download)
  (guix packages)
  (gnu packages bash)
  (gnu packages crates-io)
  (gnu packages rust-apps)
  (gnu packages crates-graphics)
  (gnu packages documentation)
  (gnu packages fontutils)
  (gnu packages version-control)
  (gnu packages file)
  (gnu packages linux)
  (gnu packages gawk)
  (gnu packages compression)
  (gnu packages autotools)
  (gnu packages gcc)
  (gnu packages pkg-config))


;; facke quickcheck needed by cargo build in order to create a lock file
;; https://github.com/rust-lang/cargo/issues/4544
(define-public rust-quickcheck-1.0.3
    (package
      (name "quickcheck")
      (version "1.0.3")
      (source
       (origin
         (method url-fetch)
         (uri "../../../../target/package/quickcheck-1.0.3.crate")
         (sha256
          (base32 "0xzi3zldq1j9b45339w28d8snmri42fcdsgszni512nqxq8qj81k"))))
      (build-system cargo-build-system)

      (home-page "..")
      (synopsis "..")
      (description
       "..")
      (license gpl3+))
    )

;; facke quickcheck_macros needed by cargo build in order to create a lock file
;; https://github.com/rust-lang/cargo/issues/4544
(define-public rust-quickcheck_macros-1.0.0
    (package
      (name "quickcheck_macros")
      (version "1.0.0")
      (source
       (origin
         (method url-fetch)
         (uri "../../../../target/package/quickcheck_macros-1.0.0.crate")
         (sha256
          (base32 "048zmv6grqy8xasivsbzrbilwyjzxwakaws0i8j9yp74x2aim2k1"))))
      (build-system cargo-build-system)

      (home-page "..")
      (synopsis "..")
      (description
       "..")
      (license gpl3+))
    )

(define-public rust-binary_codec_sv2-0.1.2
    (package
      (name "rust-binary_codec_sv2")
      (version "0.1.2")
      (source
       (origin
         (method url-fetch)
         (uri "../../../../target/package/binary_codec_sv2-0.1.2.crate")
         (sha256
          (base32 "127i53c8zyh9wxy568zn1rh80k4lblvswjpfvj7dvjbaqm4fzlpc"))))
      (build-system cargo-build-system)
        (arguments
         `(
           #:cargo-inputs
           (("quickcheck" , rust-quickcheck-1.0.3)
            )))

      (home-page "..")
      (synopsis "..")
      (description
       "..")
      (license gpl3+))
    )

(define-public rust-derive_codec_sv2-0.1.2
    (package
      (name "rust-derive_codec_sv2")
      (version "0.1.2")
      (source
       (origin
         (method url-fetch)
         (uri "../../../../target/package/derive_codec_sv2-0.1.2.crate")
         (sha256
          (base32 "0sb9zqa7l4mmkkslpygmir8cisjkm6mf09rr53vsq33ykj3009a5"))))
      (build-system cargo-build-system)
        (arguments
         `(
           #:cargo-inputs
           (("binary_codec_sv2" , rust-binary_codec_sv2-0.1.2)
            )))

      (home-page "..")
      (synopsis "..")
      (description
       "..")
      (license gpl3+))
    )
(define-public rust-binary_sv2-0.1.4
    (package
      (name "rust-binary_sv2")
      (version "0.1.4")
      (source
       (origin
         (method url-fetch)
         (uri "../../../../target/package/binary_sv2-0.1.4.crate")
         (sha256
          (base32 "087hlqkfnijmna1nic1dmxmvidabnkyr4nyad0h226jkjjslhvhp"))))
      (build-system cargo-build-system)
        (arguments
         `(
           #:skip-build? #t
           #:cargo-inputs
           (
            ("binary_codec_sv2" , rust-binary_codec_sv2-0.1.2)
            ("derive_codec_sv2" , rust-derive_codec_sv2-0.1.2)
            )))

      (home-page "..")
      (synopsis "..")
      (description
       "..")
      (license gpl3+))
    )
(define-public rust-const_sv2-0.1.0
    (package
      (name "rust-const_sv2")
      (version "0.1.0")
      (source
       (origin
         (method url-fetch)
         (uri "../../../../target/package/const_sv2-0.1.0.crate")
         (sha256
          (base32 "1qgr8fpjza8dk7r60nrnaicyazz75w9a99yq34rsv6x8finyrbdi"))))
      (build-system cargo-build-system)
      (home-page "..")
      (synopsis "..")
      (description
       "..")
      (license gpl3+))
    )
(define-public rust-framing_sv2-0.1.3
    (package
      (name "rust-framing_sv2")
      (version "0.1.3")
      (source
       (origin
         (method url-fetch)
         (uri "../../../../target/package/framing_sv2-0.1.3.crate")
         (sha256
          (base32 "06z8lw10a85cgbx8lw0v6wggddr7w0463vcc05h11rc57cjpbir6"))))
      (build-system cargo-build-system)
        (arguments
         `(
           #:skip-build? #t
           #:cargo-inputs
           (
            ("binary_sv2" , rust-binary_sv2-0.1.4)
            ("const_sv2" , rust-const_sv2-0.1.0)
            )))

      (home-page "..")
      (synopsis "..")
      (description
       "..")
      (license gpl3+))
    )
(define-public rust-codec_sv2-0.1.4
    (package
      (name "rust-codec_sv2")
      (version "0.1.4")
      (source
       (origin
         (method url-fetch)
         (uri "../../../../target/package/codec_sv2-0.1.4.crate")
         (sha256
          (base32 "1nb3v4p50g6r88i9649sgbil4x8ahlhzzxf2inaisxqpwq84f1vh"))))
      (build-system cargo-build-system)
        (arguments
         `(
           #:skip-build? #t
           #:cargo-inputs
           (
            ("binary_sv2" , rust-binary_sv2-0.1.4)
            ("const_sv2" , rust-const_sv2-0.1.0)
            ("framing-sv2" , rust-framing_sv2-0.1.3)
            )))

      (home-page "..")
      (synopsis "..")
      (description
       "..")
      (license gpl3+))
    )
(define-public rust-common_messages_sv2-0.1.4
    (package
      (name "rust-common_messages_sv2")
      (version "0.1.4")
      (source
       (origin
         (method url-fetch)
         (uri "../../../../target/package/common_messages_sv2-0.1.4.crate")
         (sha256
          (base32 "0f1ddwifc2vk0144dsnkayrf4rfc4hcg19gjf0f38j562w7vidyy"))))
      (build-system cargo-build-system)
        (arguments
         `(
           #:skip-build? #t
           #:cargo-inputs
           (
            ("binary_sv2" , rust-binary_sv2-0.1.4)
            ("const_sv2" , rust-const_sv2-0.1.0)
            ("quickcheck" , rust-quickcheck-1.0.3)
            ("quickcheck_macros" , rust-quickcheck_macros-1.0.0)
            )))

      (home-page "..")
      (synopsis "..")
      (description
       "..")
      (license gpl3+))
    )

(define-public rust-template_distribution_sv2-0.1.4
    (package
      (name "rust-template_distribution_sv2")
      (version "0.1.4")
      (source
       (origin
         (method url-fetch)
         (uri "../../../../target/package/template_distribution_sv2-0.1.4.crate")
         (sha256
          (base32 "0ic0ix5sbliv3kg4ai16lc0p87vjlndyhwgv9vhn38w1h6f4wc3z"))))
      (build-system cargo-build-system)
        (arguments
         `(
           #:skip-build? #t
           #:cargo-inputs
           (
            ("binary_sv2" , rust-binary_sv2-0.1.4)
            ("const_sv2" , rust-const_sv2-0.1.0)
            ("quickcheck" , rust-quickcheck-1.0.3)
            ("quickcheck_macros" , rust-quickcheck_macros-1.0.0)
            )))

      (home-page "..")
      (synopsis "..")
      (description
       "..")
      (license gpl3+))
    )

(define-public rust-sv2_ffi-0.1.3
    (package
      (name "rust-sv2_ffi")
      (version "0.1.3")
      (source
       (origin
         (method url-fetch)
         (uri "../../../../target/package/sv2_ffi-0.1.3.crate")
         (sha256
          (base32 "10j6396f3wa5x2zs65zdgzmaxvcwka59masrm0p235zl5i973zmf"))))
      (build-system cargo-build-system)
      (arguments
       `(
         #:cargo-inputs
         (
          ("binary_sv2" , rust-binary_sv2-0.1.4)
          ("quickcheck" , rust-quickcheck-1.0.3)
          ("quickcheck_macros" , rust-quickcheck_macros-1.0.0)
          ("codec_sv2" , rust-codec_sv2-0.1.4)
          ("const_sv2" , rust-const_sv2-0.1.0)
          ("common-messages_sv2" , rust-common_messages_sv2-0.1.4)
          ("template-distribution_sv2" , rust-template_distribution_sv2-0.1.4)
          )
         #:phases
         (modify-phases %standard-phases
              (replace 'install
                (lambda* (#:key inputs outputs skip-build? features install-source? #:allow-other-keys)
                  (let* ((out (assoc-ref outputs "out")))
                    (mkdir-p out)
                
                    (install-file "./sv2.h" out)
                    (install-file "./target/release/libsv2_ffi.a" out)
                  #true)))
              )
       ))
      (home-page "..")
      (synopsis "..")
      (description
       "..")
      (license gpl3+))
    )

(packages->manifest
 (append
  (list ;; The Basics
        bash
        which
        coreutils
        util-linux
        ;;; File(system) inspection
        file
        grep
        diffutils
        findutils
        ;;; File transformation
        patch
        gawk
        sed
        ;;; Compression and archiving
        tar
        bzip2
        gzip
        xz
        zlib
        ;;; Build tools
        gnu-make
        libtool
        autoconf
        automake
        pkg-config
        ;rust-sv2_example
        rust-quickcheck-1.0.3
        rust-quickcheck_macros-1.0.0
        rust-binary_codec_sv2-0.1.2
        rust-derive_codec_sv2-0.1.2
        rust-binary_sv2-0.1.4
        rust-const_sv2-0.1.0
        rust-framing_sv2-0.1.3
        rust-codec_sv2-0.1.4
        rust-common_messages_sv2-0.1.4
        rust-template_distribution_sv2-0.1.4
        rust-sv2_ffi-0.1.3
        gcc
  )))

