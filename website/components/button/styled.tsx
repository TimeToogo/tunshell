import styled, { css } from "styled-components";

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
  color: #fff;
  background-color: #271f1c;
  font-size: inherit;

  &:hover {
    color: #231c1c;
    background: #bd9898;
  }

  &:active {
    transform: translate(2px, 2px);
    box-shadow: none;
  }

  ${(props) =>
    props.mode === "inverted" &&
    css`
      color: #231c1c;
      background: #bd9898;

      &:hover {
        color: #fff;
        background-color: #271f1c;
      }
    `}
`;
