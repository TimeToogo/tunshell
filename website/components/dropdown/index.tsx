import React, { HTMLAttributes, useRef, useEffect } from "react";
import { DropdownContainer } from "./styled";

interface Props {
  onSelect: (string) => void;
  inline?: boolean;
  disabled?: boolean;
}

export const Dropdown: React.FC<
  React.DetailedHTMLProps<HTMLAttributes<HTMLSelectElement>, HTMLSelectElement> & Props
> = ({ children, onSelect, inline, disabled, ...props }) => {
  const selectRef = useRef();

  useEffect(() => {
    if (!selectRef.current) {
      return;
    }

    const easydropdown = require("easydropdown").default;

    const edd = easydropdown(selectRef.current, {
      callbacks: {
        onSelect: (value) => onSelect(value),
      },
    });

    return () => {
      edd.destroy();
    };
  }, []);

  return (
    <DropdownContainer inline={inline} disabled={disabled}>
      <select {...props} ref={selectRef}>
        {children}
      </select>
    </DropdownContainer>
  );
};
