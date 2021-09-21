(use-modules (gnu)
  (gnu packages bash)
  (gnu packages rust)
  )

(packages->manifest
 (append
  (list ;; The Basics
        bash
        coreutils
        ;;; Build tools
        rust-1.51
  )))

