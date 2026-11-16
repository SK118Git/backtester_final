(in-package :cl-user)

(defun read-ir-from-stream (stream)
  "Read exactly ONE Ir form from stream as pure data. 
   A .strat file holds a single strategy signal if a second form follows."
  (let ((*read-eval* nil)
	(*read-default-float-format* 'double-float))
    (let ((form (read stream nil :eof)))
      (when (eq form :eof)
	(error "EMPTY IR: nothing to read."))
      (let ((extra (read stream nil :eof)))
	(unless (eq extra :eof)
	  (error "Unexpected form after the strategy: ~S" extra)))
      form)
   )
)


(defun read-ir-from-string (string)
  "Read one IR from STRING. Handy for tests and for the LLM's candidate IR"
  (with-input-from-string (in string)
    (read-ir-from-stream in)
    )
)

(defun read-ir-from-file (path)
  "Read one IR for from the file at PATH"
  (with-open-file (in path :direction :input)
    (read-ir-from-stream in)
    )
)

(defun check-rule (text)
  (let ((*errors* '()) (*free-vars* '()))
    (validate-rule (read-ir-from-string text))
    (format t "errors:    ~A~%" (nreverse *errors*))
    (format t "free-vars: ~A~%" (nreverse *free-vars*))))
