(in-package :cl-user)

(defun bind-tree (node bindings)
  "Replace each free-var symbol in node with its value from bindings"
  (cond 
    ((consp node)
     (mapcar (lambda (x) (bind-tree x bindings)) node))
    ((and (symbolp node) (assoc node bindings))
     (cdr (assoc node bindings)))
    (t node)))

(defun check-binding (free-vars bindings)
  (let ((problems '()))
    (dolist (v free-vars)
      (unless (assoc v bindings)
	(push (format nil "unbound parameter: ~A" v) problems)))
    (dolist (pair bindings)
      (unless (member (car pair) free-vars)
	(push (format nil "binding for unknown parameter: ~A" (car pair)) problems)))
    (nreverse problems)))

(defun bind (ast bindings)
  "Substitute bindings into ast's free vars; return bound IR text for the engine."
  (let ((*print-case* :downcase)
	(*read-default-float-format* 'double-float))
    (write-to-string  (bind-tree ast bindings) :pretty nil)))
