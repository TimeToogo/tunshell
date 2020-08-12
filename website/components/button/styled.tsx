import styled, { css } from "styled-components";
import { COLOURS } from "../../theme/colours";

export const Button = styled.button<{ mode?: string }>`
  border: none;
  padding: 15px;
  text-decoration: none;
  display: inline-flex;
  border-radius: 8px;
  border: 1px solid transparent;
  box-shadow: 2px 2px 1px 1px rgba(0, 0, 0, 0.5);
  box-sizing: border-box;
  cursor: pointer;
  transition: 0.2s ease-out all;
  outline: 0 !important;
  color: ${COLOURS.WHITE};
  background-color: ${COLOURS.TAN2};
  font-size: inherit;

  &:hover {
    color: ${COLOURS.TAN2};
    background: ${COLOURS.TAN4};
  }

  &:active {
    transform: translate(2px, 2px);
    box-shadow: none;
  }

  &:disabled {
    cursor: default;
    background: #888 !important;
    color: ${COLOURS.OFF_BLACK}!important;
    opacity: 0.5;
  }

  ${(props) =>
    props.mode === "inverted" &&
    css`
      color: ${COLOURS.TAN2};
      background: ${COLOURS.TAN4};

      &:hover {
        color: ${COLOURS.WHITE};
        background-color: ${COLOURS.TAN2};
      }
    `}
`;
