import styled, { css } from "styled-components";
import { COLOURS } from "../../theme/colours";

export const TermViewport = styled.div`
  width: 100%;
  height: 100%;
  box-sizing: border-box;
`;

export const Term = styled.div`
  width: 100%;
  height: 100%;
  background: ${COLOURS.BLACK};
  box-shadow: 0 0 5px #222;
  z-index: 11;
  border-radius: 10px;
  padding: 20px;
  position: relative;
  box-sizing: border-box;
`;

export const FullScreenWrapper = styled.div`
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  z-index: 10;
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
  justify-content: center;
  align-items: center;
  padding: 20px;

  ${Term} {
    height: 400px;
    width: 900px;
    min-height: 380px;
    max-height: 100%;
    max-width: 100%;
    margin: 20px;
  }
`;

export const Overlay = styled.div`
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  z-index: 9;
  background: rgba(51, 47, 45, 0.75);
`;

export const Close = styled.button`
  position: absolute;
  top: -25px;
  right: -20px;
  font-size: 40px;
  padding: 0;
  border-radius: 50px;
  border: none;
  background: ${COLOURS.TAN4};
  color: ${COLOURS.TAN2};
  cursor: pointer;
  z-index: 10;
  display: flex;
  justify-content: center;
  align-items: center;
  outline: 0 !important;
`;
