import * as React from "react";
import { cn } from "../../helpers/utils";
import { Expand, Minimize } from "lucide-react";

export interface TextareaProps
  extends React.TextareaHTMLAttributes<HTMLTextAreaElement> {
  isExpanded?: boolean;
  onToggleExpand?: () => void;
}

const Textarea = React.forwardRef<HTMLTextAreaElement, TextareaProps>(
  (
    { className, isExpanded, onToggleExpand, onChange, value, ...props },
    ref
  ) => {
    // Create a memoized handler for input events
    const handleInputChange = React.useCallback(
      (
        e:
          | React.ChangeEvent<HTMLTextAreaElement>
          | React.FormEvent<HTMLTextAreaElement>
      ) => {
        if (onChange && "target" in e) {
          const event = new Event("change", { bubbles: true });
          Object.defineProperty(event, "target", { value: e.target });
          onChange(event as unknown as React.ChangeEvent<HTMLTextAreaElement>);
        }
      },
      [onChange]
    );

    return (
      <div className="relative w-full">
        <button
          onClick={onToggleExpand}
          className="absolute top-2 right-2 text-muted-foreground hover:text-foreground z-10"
          type="button"
        >
          {isExpanded ? (
            <Minimize className="h-4 w-4" />
          ) : (
            <Expand className="h-4 w-4" />
          )}
        </button>
        <textarea
          className={cn(
            "flex w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50",
            isExpanded
              ? "h-[66vh] max-h-[66vh]"
              : "min-h-[45px] max-h-[100px] md:min-h-[80px] md:max-h-[200px]",
            className
          )}
          ref={ref}
          spellCheck="true"
          autoComplete="on"
          lang="auto"
          dir="auto"
          translate="yes"
          value={value}
          onChange={handleInputChange}
          onInput={handleInputChange}
          onCompositionEnd={handleInputChange}
          {...props}
        />
      </div>
    );
  }
);
Textarea.displayName = "Textarea";

export { Textarea };
