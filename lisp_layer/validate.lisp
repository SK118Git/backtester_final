(in-package :cl-user)

;; Closed vocab defined by grammar
(defparameter *px-fields* '(bid ask mid spread bid-vol ask-vol))
(defparameter *state-fields* '(position-size avg-entry-price high-water-mark low-water-mark entry-time num-entries num-exits unrealized-pnl time-in-trade trailing-dist))
(defparameter *account-fields* '(equity cash realized-pnl market-value))
(defparameter *indicators* '(sma ema rsi vol)) ;; TODO add more/extensible

;; operator -> (operand-type result-type min-args max-args) max = nil => unbounded
(defparameter *operators*
  '((> :num :bool 2 2) (>= :num :bool 2 2)
    (< :num :bool 2 2) (<= :num :bool 2 2) (= :num :bool 2 2)
    (and :bool :bool 1 nil) (or :bool :bool 1 nil) (not :bool :bool 1 1)
    (+ :num :num 1 nil) (* :num :num 1 nil)
    (- :num :num 1 2) (/ :num :num 2 2)
    ;; We decided to have the nested unfolding be any directional but - and / are not associative so they can't be variadic 
    ;; - unary exists because of UnOp::Neg, there is no UnOp::Recip for (/ x ) to resolve to anything
    )
)

;; Accumulators which will be rebound dynamically
(defvar *errors* nil)
(defvar *free-vars* nil)

(defun add-error (control &rest args)
  "Record one formatted error message onto *errors*"
  (push (apply #'format nil control args) *errors*))

;; Signal names available for current run, dynamically bound, nil if not yet. 
(defvar *signals* nil)
(defun known-signal-p (name)
  "Is NAME a valid signal for this run? 
   Until we populate *signals* anything is accepted"
  (or (null *signals*)
      (member name *signals*)))


(defun validate-expr (e)
  "Validate expression E, recording errors/free-vars. Return its type (:num or :bool)"
  (cond
    ((numberp e) :num)
    ((eq e 'true) :bool)
    ((eq e 'false) :bool)
    ((symbolp e) (pushnew e *free-vars*) :num)
    ((consp e) (validate-compound e))
    (t (add-error "Not an expression: ~S" e) :num)))


(defun validate-compound (e)
  (let ((spec (assoc (first e) *operators*)))
    (if spec
	(validate-operator e (rest spec))
	(case (first e)
	  (px (check-field e *px-fields* "price"))
	  (state (check-field e *state-fields* "state"))
	  (account (check-field e *account-fields* "account"))
	  (signal 
	   (let ((name (second e)))
	     (unless (= (length e) 2)
	       (add-error "signal takes one name, got ~S" e))
	     (unless (known-signal-p name)
	       (add-error "Unknown signal ~S; expected one of ~S" name *signals*))
	     :num))
	  (if (cond 
		((/= (length e) 4)
		 (add-error "if takes cond/then/else, got ~S" e)
		 :num)
		(t 
		 (expect (second e) :bool "if condition")
		 (let ((then-type (validate-expr (third e)))
		       (else-type (validate-expr (fourth e))))
		   (unless (eq then-type else-type)
		     (add-error "if branches disagree: then is ~A, else is ~A in ~S" then-type else-type e))
		   then-type))))
	  (otherwise 
	   (let ((op (first e)))
	     (cond 
	       ((member op *indicators* )
		(if (= (length e) 2)
		    (expect (second e) :num op)
		    (add-error "~S takes one period argument, got ~S" op e))
		:num)
	       (t 
		(add-error "Unknown operator ~S" op)
		:num))))
	  ))))




(defun check-field (e allowed what)
  "E is (op field); error unless field is in ALLOWED. What names the kind"
  (unless (= (length e) 2)
    (add-error "~A access takes one field, got ~S" what e))
  (unless (member (second e) allowed)
    (add-error "Unknown ~A field ~S; expected one of ~S" what (second e) allowed))
  :num)


(defun expect (e wanted ctx)
  (let ((got (validate-expr e)))
    (unless (eq got wanted)
      (add-error "~A: wanted ~A, got ~A, in ~S" ctx wanted got e))
    got))


(defun validate-operator (e spec)
  "SPEC is (operand-type result-type min max). Validate E's operands against it."
  (destructuring-bind (operand-type result-type min max) spec
    (let* ((args (rest e))
	   (n (length args)))
      (when (< n min) (add-error "~S needs at least ~A args, got ~A" (first e) min n))
      (when (and max (> n max)) (add-error "~S takes at most ~A args, got ~A" (first e) max n))
      (dolist (a args)
	(expect a operand-type (first e)))
      result-type)))


(defun validate-size (size allowed ctx)
  "Size should be (kind amount) with kind in allowed.
   Amount is a :num and carried per-action legality rule"
  (cond 
    ((not (consp size))
     (add-error "~A: size must be (units/fraction/risk-pct amount), got ~S" ctx size))
    ((not (member (first size) allowed))
     (add-error "~A: ~S not allowed here; expected one of ~S" ctx (first size) allowed))
    ((/= (length size) 2)
     (add-error "~A: size takes one amount, got ~S" ctx size))
    (t 
     (expect (second size) :num ctx))))

(defun validate-rule (r)
  "validate one rule: (rule (when EXPR) (then ACTION...))."
  (cond 
    ((not (and (consp r) (eq (first r) 'rule)))
     (add-error "Not a rule ~S" r))
    (t 
     (let ((when-clause (second r))
	   (then-clause (third r)))
       (if (and (consp when-clause) (eq (first when-clause) 'when))
	   (expect (second when-clause) :bool "rule condition")
	   (add-error "Rule missing a (when ...) clause: ~S" r))
       (if (and (consp then-clause) (eq (first then-clause) 'then))
	   (let ((actions (rest then-clause)))
	     (cond 
	       ((null actions)
		(add-error "Rule has no actions: ~S" r))
	       (t 
		(dolist (a actions) (validate-action a))
		(check-action-sequence actions r))))
	   (add-error "Rule missing a (then ...) clause: ~S" r))))))

(defun validate-action (a)
  "Validate a single action."
  (if (not (consp a))
      (add-error "Not an action: ~S" a)
      (case (first a)
        (enter
         (cond
           ((/= (length a) 3)
            (add-error "enter takes a side and a size: ~S" a))
           (t
            (unless (member (second a) '(long short))
              (add-error "enter side must be long or short, got ~S" (second a)))
            (validate-size (third a) '(units risk-pct) "enter"))))
        (exit
         (cond
           ((/= (length a) 2)
            (add-error "exit takes one size (or all): ~S" a))
           ((eq (second a) 'all) nil)
           (t
            (validate-size (second a) '(units fraction) "exit"))))
        (set-stop
         (cond
           ((/= (length a) 3)
            (add-error "set-stop takes a kind and a level: ~S" a))
           (t
            (unless (member (second a) '(fixed trailing))
              (add-error "set-stop kind must be fixed or trailing, got ~S" (second a)))
            (expect (third a) :num "set-stop level"))))
        (set-target
         (cond
           ((/= (length a) 2)
            (add-error "set-target takes a level: ~S" a))
           (t
            (expect (second a) :num "set-target level"))))
        (cancel-stop
         (unless (= (length a) 1)
           (add-error "cancel-stop takes no arguments: ~S" a)))
        (cancel-target
         (unless (= (length a) 1)
           (add-error "cancel-target takes no arguments: ~S" a)))
        (otherwise
         (add-error "Unknown action ~S" (first a))))))



(defun risk-pct-size-p (size)
  "is size a (risk-pct ...) form?"
  (and (consp size) (eq (first size) 'risk-pct)))


(defun check-action-sequence (actions rule)
  "Order dependent cross-action rules over one rule's action list:
   a risk-pct enter needs a set-stop before it; no stop/target after (exit all)"
  (let ((have-stop nil)
	(flat nil))
    (dolist (a actions)
      (when (consp a)
	(case (first a)
	  (enter 
	   (when (risk-pct-size-p (third a))
	     (unless have-stop
	       (add-error "risk-pct enter needs a set-stop before it: ~S in ~S" a rule)))
	   (setf flat nil))
	  (set-stop
	   (when flat 
	     (add-error "set-stop after (exit all): ~S in ~S" a rule))
	   (setf have-stop t))
	  (set-target 
	   (when flat 
	     (add-error "set-target after (exit all): ~S in ~S" a rule)))
	  (cancel-stop 
	   (setf have-stop nil))
	  (exit 
	   (when (eq (second a) 'all)
	     (setf flat t))))))))

(defun validate-strategy (s)
  "Validate a (strategy RULE ...) form."
  (cond 
    ((not (and (consp s) (eq (first s) 'strategy)))
     (add-error "Not a strategy: ~S" s))
    ((null (rest s))
     (add-error "Strategy has no rules: ~S" s))
    (t 
     (dolist (r (rest s))
       (validate-rule r)))))

(defun validate (ast) 
  "Validate a strategy AST. Returns (values ok-p errors free-vars)."
  (let ((*errors* nil)
	(*free-vars* nil))
    (validate-strategy ast)
    (values (null *errors*)
	    (nreverse *errors*)
	    (nreverse *free-vars*))))
