import React, { HTMLAttributes, useRef, useEffect } from "react";
import { DropdownContainer } from "./styled";

export const Dropdown: React.FC<HTMLAttributes<HTMLSelectElement> & { onSelect: (string) => void }> = ({
  children,
  onSelect,
  ...props
}) => {
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
    <DropdownContainer>
      <select ref={selectRef}>{children}</select>
    </DropdownContainer>
  );
};
