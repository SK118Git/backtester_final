(in-package :cl-user)


(defparameter *op-words* 
  '((> . "is above") (>= . "is at least")
    (< . "is below") (<= . "is at most") (= . "equals")
    (and . "and") (or . "or")
    (+ . "plus") (- . "minus") 
    (* . "times") (/ . "divided by")))

(defparameter *px-phrases* 
  '((bid . "the bid price") (ask . "the ask price") (mid . "the mid price")
    (spread . "the spread") (bid-vol . "the bid volume") (ask-vol . "the ask volume")))

(defparameter *state-phrases* 
  '((position-size . "the position size") (avg-entry-price . "the average entry price")
    (high-water-mark . "the high water-mark") (low-water-mark . "the low water-mark")
    (entry-time . "the entry time") (num-entries . "the number of entries") 
    (num-exits . "the number of exits") (unrealized-pnl . "the unrealized PnL")
    (time-in-trade . "the time in trade") (trailing-dist . "the trailing distance")))


(defparameter *account-phrases*
  '((equity . "the account equity") (cash . "the cash balance") 
    (realized-pnl . "the realized PnL") (market-value . "the market value")))

(defun join (strings separator)
  "Concatenate strings with separator between consecutive ones"
  (format nil (concatenate 'string "~{~A~^" separator "~}") strings))


(defun readback-expr (e) 
  "Render expression E as English"
  (cond 
    ((numberp e) (format nil "~A" e))
    ((eq e 'true) "true")
    ((eq e 'false) "false")
    ((symbolp e) (format nil "~A" e))
    ((consp e) (readback-compound e))
    (t (format nil "~S" e))))

(defun readback-compound (e)
  (case (first e)
    (px (cdr (assoc (second e) *px-phrases*)))
    (state (cdr (assoc (second e) *state-phrases*)))
    (account (cdr (assoc (second e) *account-phrases*)))
    (signal (format nil "the ~A signal" (second e)))
    ((> >= = <= < and or + * /)
     (format nil "(~A)" (join (mapcar #'readback-expr (rest e))
			      (format nil " ~A " (cdr (assoc (first e) *op-words*))))))
    (- (if (= (length e) 2) 
	   (format nil "the opposite of ~A" (readback-expr (second e)))
	   (format nil "(~A)" (join (mapcar #'readback-expr (rest e)) " minus "))))
    (not (format nil "not ~A" (readback-expr (second e))))
    (if (destructuring-bind (op test then else) e 
	  (declare (ignore op))
	  (format nil "(if ~A then ~A, otherwise ~A)" 
		  (readback-expr test)
		  (readback-expr then)
		  (readback-expr else))))
    (otherwise (format nil "the ~A over ~A" (first e) (readback-expr (second e)))) ;; an indicator
   ))

(defun readback-size (size)
  "Render a size form (units/fraction/risk-pct AMOUNT) as english" 
  (destructuring-bind (kind amount) size 
    (case kind 
      (units (format nil "~A units" (readback-expr amount)))
      (fraction (format nil "a fraction ~A of the position" (readback-expr amount)))
      (risk-pct (format nil "risking ~A of equity" (readback-expr amount))))))

(defun readback-action (a)
  (case (first a)
    (enter (format nil "enter ~(~A~), ~A" (second a) (readback-size (third a))))
    (exit (if (eq (second a) 'all)
	      "close the position"
	      (format nil "exit ~A" (readback-size (second a)))))
    (set-stop (format nil "set a ~(~A~) stop at ~A"
		      (second a) (readback-expr (third a))))
    (set-target (format nil "set a target at ~A" (readback-expr (second a))))
    (cancel-stop "cancel the stop")
    (cancel-target "cancel the target")))

(defun readback-rule (r)
  (destructuring-bind (op when-clause then-clause) r 
    (declare (ignore op))
    (format nil "When ~A: ~A."
	    (readback-expr (second when-clause))
	    (join (mapcar #'readback-action (rest then-clause)) "; "))))

(defun readback-strategy (s) 
  (let ((rules (rest s)))
    (format nil "Strategy with ~A rule~:P (checked in order; the first whose condition holds fires):~%~%~{~A~%~}"
	    (length rules)
	    (mapcar #'readback-rule rules))))

(defun readback (ast)
  "Render a validated strategy AST as English"
  (readback-strategy ast))
