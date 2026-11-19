(in-package :cl-user)

(dolist (f '("reader" "validate" "readback" "bind"))
  (load (merge-pathnames (make-pathname :name f :type "lisp") *load-pathname*)))

(defun read-binding (string)
  "Read a binding alist from a string"
  (let ((*read-eval* nil)
	( *read-default-float-format* 'double-float))
    (read-from-string string)))

(defun run-validate ()
  (let ((*print-pretty* nil))
    (multiple-value-bind (ok errors free-vars)
	(validate (read-ir-from-stream *standard-input*))
      (cond 
	(ok (format t "OK~%~{~A~^ ~}~%" free-vars) 0)
	(t (format t "ERRORS~%~{~A~%~}" errors) 1)))))


(defun run-readback () 
  (write-line (readback (read-ir-from-stream *standard-input*)))
  0)


(defun run-bind (binding-arg)
  (unless binding-arg 
    (error "bind needs a binding argument"))
  (write-line (bind (read-ir-from-stream *standard-input*)
		    (read-binding binding-arg)))
  0)


(defun cli-main ()
  (let* ((args (rest sb-ext:*posix-argv*)) ;; args after program name
	 (verb (first args)))
    (handler-case 
	(sb-ext:exit 
	 :code (cond 
		 ((equal verb "validate") (run-validate))
		 ((equal verb "readback") (run-readback))
		 ((equal verb "bind") (run-bind (second args)))
		 (t (format *error-output*  "usage: validate | readback | bind <binding>~%")
		    2)))
      (error (e)
	(format *error-output* "error: ~A~%" e)
	(sb-ext:exit :code 1)))))


(cli-main)
