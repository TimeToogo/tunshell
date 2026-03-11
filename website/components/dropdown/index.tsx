import React, { PropsWithChildren, SelectHTMLAttributes, useRef, useEffect } from "react";
import { DropdownContainer } from "./styled";

interface Props extends Omit<SelectHTMLAttributes<HTMLSelectElement>, "onSelect"> {
  onSelect: (value: string) => void;
  inline?: boolean;
  disabled?: boolean;
}

type DropdownProps = PropsWithChildren<Props>;

export const Dropdown = ({ children, onSelect, inline, disabled, ...props }: DropdownProps) => {
  const selectRef = useRef<HTMLSelectElement | null>(null);

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
      <select {...props} disabled={disabled} ref={selectRef}>
        {children}
      </select>
    </DropdownContainer>
  );
};
